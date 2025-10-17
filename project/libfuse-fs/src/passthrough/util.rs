// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE-BSD-3-Clause file.
// Copyright (C) 2023 Alibaba Cloud. All rights reserved.

use std::collections::{BTreeMap, btree_map};
use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use super::os_compat::{ino64_t, stat64};
use rfuse3::raw::reply::FileAttr;
use rfuse3::{FileType, Timestamp};
use tracing::error;

use super::inode_store::InodeId;
use super::{CURRENT_DIR_CSTR, EMPTY_CSTR, MAX_HOST_INO, PARENT_DIR_CSTR};

// Platform-specific constants
// Linux-specific flags that don't exist on macOS
#[cfg(target_os = "linux")]
pub const O_PATH_OR_RDONLY: i32 = libc::O_PATH;
#[cfg(target_os = "macos")]
pub const O_PATH_OR_RDONLY: i32 = libc::O_RDONLY;

#[cfg(target_os = "linux")]
pub const AT_EMPTY_PATH_FLAG: i32 = libc::AT_EMPTY_PATH;
#[cfg(target_os = "macos")]
pub const AT_EMPTY_PATH_FLAG: i32 = 0; // macOS doesn't support AT_EMPTY_PATH

#[cfg(target_os = "linux")]
pub const O_DIRECT_FLAG: i32 = libc::O_DIRECT;
#[cfg(target_os = "macos")]
pub const O_DIRECT_FLAG: i32 = 0; // macOS doesn't support O_DIRECT

// System call numbers
#[cfg(target_os = "linux")]
pub const SYS_GETDENTS64: i32 = libc::SYS_getdents64;
#[cfg(target_os = "macos")]
pub const SYS_GETDENTS64: i32 = 0; // Not used on macOS, we use getdirentries instead

/// the 56th bit used to set the inode to 1 indicates virtual inode
const VIRTUAL_INODE_FLAG: u64 = 1 << 55;

/// Used to form a pair of dev and mntid as the key of the map
#[derive(Clone, Copy, Default, PartialOrd, Ord, PartialEq, Eq, Debug)]
struct DevMntIDPair(libc::dev_t, u64);

// Used to generate a unique inode with a maximum of 56 bits. the format is
// |1bit|8bit|47bit
// when the highest bit is equal to 0, it means the host inode format, and the lower 47 bits normally store no more than 47-bit inode
// When the highest bit is equal to 1, it indicates the virtual inode format,
// which is used to store more than 47 bits of inodes
// the middle 8bit is used to store the unique ID produced by the combination of dev+mntid
pub struct UniqueInodeGenerator {
    // Mapping (dev, mnt_id) pair to another small unique id
    dev_mntid_map: Mutex<BTreeMap<DevMntIDPair, u8>>,
    next_unique_id: AtomicU8,
    next_virtual_inode: AtomicU64,
}

impl UniqueInodeGenerator {
    pub fn new() -> Self {
        UniqueInodeGenerator {
            dev_mntid_map: Mutex::new(Default::default()),
            next_unique_id: AtomicU8::new(1),
            next_virtual_inode: AtomicU64::new(1),
        }
    }

    pub fn get_unique_inode(&self, id: &InodeId) -> io::Result<ino64_t> {
        let unique_id = {
            let id: DevMntIDPair = DevMntIDPair(id.dev, id.mnt);
            let mut id_map_guard = self.dev_mntid_map.lock().unwrap();
            match id_map_guard.entry(id) {
                btree_map::Entry::Occupied(v) => *v.get(),
                btree_map::Entry::Vacant(v) => {
                    if self.next_unique_id.load(Ordering::Relaxed) == u8::MAX {
                        return Err(io::Error::other(
                            "the number of combinations of dev and mntid exceeds 255",
                        ));
                    }
                    let next_id = self.next_unique_id.fetch_add(1, Ordering::Relaxed);
                    v.insert(next_id);
                    next_id
                }
            }
        };

        let inode = if id.ino <= MAX_HOST_INO {
            id.ino
        } else {
            if self.next_virtual_inode.load(Ordering::Relaxed) > MAX_HOST_INO {
                return Err(io::Error::other(format!(
                    "the virtual inode excess {MAX_HOST_INO}"
                )));
            }
            self.next_virtual_inode.fetch_add(1, Ordering::Relaxed) | VIRTUAL_INODE_FLAG
        };

        Ok(((unique_id as u64) << 47) | inode)
    }

    #[cfg(test)]
    fn decode_unique_inode(&self, inode: libc::ino64_t) -> io::Result<InodeId> {
        use super::VFS_MAX_INO;

        if inode > VFS_MAX_INO {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("the inode {inode} excess {VFS_MAX_INO}"),
            ));
        }

        let dev_mntid = (inode >> 47) as u8;
        if dev_mntid == u8::MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid dev and mntid {dev_mntid} excess 255"),
            ));
        }

        let mut dev: libc::dev_t = 0;
        let mut mnt: u64 = 0;

        let mut found = false;
        let id_map_guard = self.dev_mntid_map.lock().unwrap();
        for (k, v) in id_map_guard.iter() {
            if *v == dev_mntid {
                found = true;
                dev = k.0;
                mnt = k.1;
                break;
            }
        }

        if !found {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid dev and mntid {dev_mntid},there is no record in memory "),
            ));
        }
        Ok(InodeId {
            ino: inode & MAX_HOST_INO,
            dev,
            mnt,
        })
    }
}

/// Safe wrapper around libc::openat().
pub fn openat(
    dir_fd: &impl AsRawFd,
    path: &CStr,
    flags: libc::c_int,
    mode: u32,
) -> io::Result<File> {
    // Safe because:
    // - CString::new() has returned success and thus guarantees `path_cstr` is a valid
    //   NUL-terminated string
    // - this does not modify any memory
    // - we check the return value
    // We do not check `flags` because if the kernel cannot handle poorly specified flags then we
    // have much bigger problems.

    let fd = if flags & libc::O_CREAT == libc::O_CREAT {
        // The mode argument is used only when O_CREAT is specified
        unsafe { libc::openat(dir_fd.as_raw_fd(), path.as_ptr(), flags, mode) }
    } else {
        unsafe { libc::openat(dir_fd.as_raw_fd(), path.as_ptr(), flags) }
    };

    println!("DEBUG: openat returned fd={}", fd);
    if fd >= 0 {
        // Safe because we just opened this fd
        Ok(unsafe { File::from_raw_fd(fd) })
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Open `/proc/self/fd/{fd}` with the given flags to effectively duplicate the given `fd` with new
/// flags (e.g. to turn an `O_PATH` file descriptor into one that can be used for I/O).
pub fn reopen_fd_through_proc(
    fd: &impl AsRawFd,
    flags: libc::c_int,
    proc_self_fd: &impl AsRawFd,
) -> io::Result<File> {
    #[cfg(target_os = "macos")]
    {
        // On macOS, we can't use /proc/self/fd, so we just duplicate the file descriptor
        let new_fd = unsafe { libc::dup(fd.as_raw_fd()) };
        if new_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(new_fd) })
    }

    #[cfg(not(target_os = "macos"))]
    {
        let name = CString::new(format!("{}", fd.as_raw_fd()).as_str())?;
        // Clear the `O_NOFOLLOW` flag if it is set since we need to follow the `/proc/self/fd` symlink
        // to get the file.
        openat(
            proc_self_fd,
            &name,
            flags & !libc::O_NOFOLLOW & !libc::O_CREAT,
            0,
        )
    }
}

pub fn stat_fd(dir: &impl AsRawFd, path: Option<&CStr>) -> io::Result<stat64> {
    // Safe because this is a constant value and a valid C string.
    let pathname =
        path.unwrap_or_else(|| unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) });
    let mut st = MaybeUninit::<stat64>::zeroed();
    let dir_fd = dir.as_raw_fd();

    // Safe because the kernel will only write data in `st` and we check the return value.
    let res = unsafe {
        #[cfg(target_os = "linux")]
        {
            libc::fstatat64(
                dir_fd,
                pathname.as_ptr(),
                st.as_mut_ptr(),
                AT_EMPTY_PATH_FLAG | libc::AT_SYMLINK_NOFOLLOW,
            )
        }
        #[cfg(target_os = "macos")]
        {
            libc::fstatat(
                dir_fd,
                pathname.as_ptr(),
                st.as_mut_ptr(),
                AT_EMPTY_PATH_FLAG | libc::AT_SYMLINK_NOFOLLOW,
            )
        }
    };

    if res >= 0 {
        // Safe because the kernel guarantees that the struct is now fully initialized.
        Ok(unsafe { st.assume_init() })
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Returns true if it's safe to open this inode without O_PATH.
pub fn is_safe_inode(mode: u32) -> bool {
    // Only regular files and directories are considered safe to be opened from the file
    // server without O_PATH.
    let mode_val = mode as libc::mode_t;
    matches!(mode_val & libc::S_IFMT, libc::S_IFREG | libc::S_IFDIR)
}

/// Returns true if the mode is for a directory.
pub fn is_dir(mode: u32) -> bool {
    let mode_val = mode as libc::mode_t;
    (mode_val & libc::S_IFMT) == libc::S_IFDIR
}

pub fn ebadf() -> io::Error {
    io::Error::from_raw_os_error(libc::EBADF)
}

pub fn einval() -> io::Error {
    io::Error::from_raw_os_error(libc::EINVAL)
}

pub fn enosys() -> io::Error {
    io::Error::from_raw_os_error(libc::ENOSYS)
}
#[allow(unused)]
pub fn eperm() -> io::Error {
    io::Error::from_raw_os_error(libc::EPERM)
}
#[allow(unused)]
pub fn convert_stat64_to_file_attr(stat: stat64) -> FileAttr {
    FileAttr {
        ino: stat.st_ino,
        size: stat.st_size as u64,
        blocks: stat.st_blocks as u64,
        atime: Timestamp::new(stat.st_atime, stat.st_atime_nsec.try_into().unwrap()),
        mtime: Timestamp::new(stat.st_mtime, stat.st_mtime_nsec.try_into().unwrap()),
        ctime: Timestamp::new(stat.st_ctime, stat.st_ctime_nsec.try_into().unwrap()),
        #[cfg(target_os = "macos")]
        crtime: Timestamp::new(0, 0), // Set crtime to 0 for non-macOS platforms
        kind: filetype_from_mode(stat.st_mode.into()),
        perm: stat.st_mode as u16 & 0o7777,
        nlink: stat.st_nlink as u32,
        uid: stat.st_uid,
        gid: stat.st_gid,
        rdev: stat.st_rdev as u32,
        #[cfg(target_os = "macos")]
        flags: 0, // Set flags to 0 for non-macOS platforms
        blksize: stat.st_blksize as u32,
    }
}

pub fn filetype_from_mode(st_mode: u32) -> FileType {
    let st_mode_val = st_mode as libc::mode_t;
    let st_mode = st_mode_val & libc::S_IFMT;
    match st_mode {
        libc::S_IFIFO => FileType::NamedPipe,
        libc::S_IFCHR => FileType::CharDevice,
        libc::S_IFBLK => FileType::BlockDevice,
        libc::S_IFDIR => FileType::Directory,
        libc::S_IFREG => FileType::RegularFile,
        libc::S_IFLNK => FileType::Symlink,
        libc::S_IFSOCK => FileType::Socket,
        _ => {
            error!("wrong st mode : {st_mode}");
            unreachable!();
        }
    }
}

/// Validate a path component. A well behaved FUSE client should never send dot, dotdot and path
/// components containing slash ('/'). The only exception is that LOOKUP might contain dot and
/// dotdot to support NFS export.
#[inline]
pub fn validate_path_component(name: &CStr) -> io::Result<()> {
    match is_safe_path_component(name) {
        true => Ok(()),
        false => Err(io::Error::from_raw_os_error(libc::EINVAL)),
    }
}
/// ASCII for slash('/')
pub const SLASH_ASCII: u8 = 47;
// Is `path` a single path component that is not "." or ".."?
fn is_safe_path_component(name: &CStr) -> bool {
    let bytes = name.to_bytes_with_nul();

    if bytes.contains(&SLASH_ASCII) {
        return false;
    }
    !is_dot_or_dotdot(name)
}
#[inline]
fn is_dot_or_dotdot(name: &CStr) -> bool {
    let bytes = name.to_bytes_with_nul();
    bytes.starts_with(CURRENT_DIR_CSTR) || bytes.starts_with(PARENT_DIR_CSTR)
}

pub fn osstr_to_cstr(os_str: &OsStr) -> Result<CString, std::ffi::NulError> {
    let bytes = os_str.as_bytes();
    let c_string = CString::new(bytes)?;
    Ok(c_string)
}

//TODO: There is a software permission issue here. But it doesn't matter at the moment
// macro_rules! scoped_cred {
//     ($name:ident, $ty:ty, $syscall_nr:expr) => {
//         #[derive(Debug)]
//         pub(crate) struct $name;

//         impl $name {
//             // Changes the effective uid/gid of the current thread to `val`.  Changes
//             // the thread's credentials back to root when the returned struct is dropped.
//             fn new(val: $ty) -> io::Result<Option<$name>> {
//                 if val == 0 {
//                     // Nothing to do since we are already uid 0.
//                     return Ok(None);
//                 }

//                 // We want credential changes to be per-thread because otherwise
//                 // we might interfere with operations being carried out on other
//                 // threads with different uids/gids.  However, posix requires that
//                 // all threads in a process share the same credentials.  To do this
//                 // libc uses signals to ensure that when one thread changes its
//                 // credentials the other threads do the same thing.
//                 //
//                 // So instead we invoke the syscall directly in order to get around
//                 // this limitation.  Another option is to use the setfsuid and
//                 // setfsgid systems calls.   However since those calls have no way to
//                 // return an error, it's preferable to do this instead.

//                 // This call is safe because it doesn't modify any memory and we
//                 // check the return value.
//                 let res = unsafe { libc::syscall($syscall_nr, -1, val, -1) };
//                 if res == 0 {
//                     Ok(Some($name))
//                 } else {
//                     Err(io::Error::last_os_error())
//                 }
//             }
//         }

//         impl Drop for $name {
//             fn drop(&mut self) {
//                 let res = unsafe { libc::syscall($syscall_nr, -1, 0, -1) };
//                 if res < 0 {
//                     error!(
//                         "fuse: failed to change credentials back to root: {}",
//                         io::Error::last_os_error(),
//                     );
//                 }
//             }
//         }
//     };
// }

// scoped_cred!(ScopedUid, libc::uid_t, libc::SYS_setresuid);
// scoped_cred!(ScopedGid, libc::gid_t, libc::SYS_setresgid);

// pub fn set_creds(
//     uid: libc::uid_t,
//     gid: libc::gid_t,
// ) -> io::Result<(Option<ScopedUid>, Option<ScopedGid>)> {
//     // We have to change the gid before we change the uid because if we change the uid first then we
//     // lose the capability to change the gid.  However changing back can happen in any order.
//     ScopedGid::new(gid).and_then(|gid| Ok((ScopedUid::new(uid)?, gid)))
// }

// Platform-specific system call wrappers
#[cfg(target_os = "linux")]
pub fn do_fdatasync(fd: libc::c_int) -> io::Result<()> {
    let ret = unsafe { libc::fdatasync(fd) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub fn do_fdatasync(fd: libc::c_int) -> io::Result<()> {
    // macOS doesn't have fdatasync, use fsync instead
    let ret = unsafe { libc::fsync(fd) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub fn do_renameat2(
    olddirfd: libc::c_int,
    oldpath: *const libc::c_char,
    newdirfd: libc::c_int,
    newpath: *const libc::c_char,
    flags: libc::c_uint,
) -> io::Result<()> {
    let ret = unsafe { libc::renameat2(olddirfd, oldpath, newdirfd, newpath, flags) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub fn do_renameat2(
    olddirfd: libc::c_int,
    oldpath: *const libc::c_char,
    newdirfd: libc::c_int,
    newpath: *const libc::c_char,
    _flags: libc::c_uint,
) -> io::Result<()> {
    // macOS doesn't have renameat2, use renameat instead
    let ret = unsafe { libc::renameat(olddirfd, oldpath, newdirfd, newpath) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub fn do_fallocate(
    fd: libc::c_int,
    mode: libc::c_int,
    offset: libc::off64_t,
    len: libc::off64_t,
) -> io::Result<()> {
    let ret = unsafe { libc::fallocate64(fd, mode, offset, len) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub fn do_fallocate(
    fd: libc::c_int,
    mode: libc::c_int,
    offset: libc::off_t,
    len: libc::off_t,
) -> io::Result<()> {
    // macOS uses fcntl with F_PREALLOCATE
    use libc::{F_PREALLOCATE, fcntl};

    #[repr(C)]
    struct fstore_t {
        fst_flags: u32,
        fst_posmode: i32,
        fst_offset: libc::off_t,
        fst_length: libc::off_t,
        fst_bytesalloc: libc::off_t,
    }

    let fstore = fstore_t {
        fst_flags: 0,
        fst_posmode: 0, // F_PEOFPOSMODE
        fst_offset: offset,
        fst_length: len,
        fst_bytesalloc: 0,
    };

    let ret = unsafe { fcntl(fd, F_PREALLOCATE, &fstore) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub fn do_lseek64(
    fd: libc::c_int,
    offset: libc::off64_t,
    whence: libc::c_int,
) -> io::Result<libc::off64_t> {
    let ret = unsafe { libc::lseek64(fd, offset, whence) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

#[cfg(target_os = "macos")]
pub fn do_lseek64(
    fd: libc::c_int,
    offset: libc::off_t,
    whence: libc::c_int,
) -> io::Result<libc::off_t> {
    let ret = unsafe { libc::lseek(fd, offset, whence) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

#[cfg(target_os = "linux")]
pub fn do_fstatvfs(fd: libc::c_int, buf: *mut libc::statvfs64) -> io::Result<()> {
    let ret = unsafe { libc::fstatvfs64(fd, buf) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub fn do_fstatvfs(fd: libc::c_int, buf: *mut libc::statvfs) -> io::Result<()> {
    let ret = unsafe { libc::fstatvfs(fd, buf) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

// Platform-specific xattr API wrappers
// macOS xattr functions have additional parameters compared to Linux
#[cfg(target_os = "linux")]
pub unsafe fn sys_getxattr(
    path: *const libc::c_char,
    name: *const libc::c_char,
    value: *mut libc::c_void,
    size: libc::size_t,
) -> libc::ssize_t {
    libc::getxattr(path, name, value, size)
}

#[cfg(target_os = "macos")]
pub unsafe fn sys_getxattr(
    path: *const libc::c_char,
    name: *const libc::c_char,
    value: *mut libc::c_void,
    size: libc::size_t,
) -> libc::ssize_t {
    unsafe { libc::getxattr(path, name, value, size, 0, 0) } // position=0, options=0
}

#[cfg(target_os = "linux")]
pub unsafe fn sys_setxattr(
    path: *const libc::c_char,
    name: *const libc::c_char,
    value: *const libc::c_void,
    size: libc::size_t,
    flags: libc::c_int,
) -> libc::c_int {
    libc::setxattr(path, name, value, size, flags)
}

#[cfg(target_os = "macos")]
pub unsafe fn sys_setxattr(
    path: *const libc::c_char,
    name: *const libc::c_char,
    value: *const libc::c_void,
    size: libc::size_t,
    flags: libc::c_int,
) -> libc::c_int {
    unsafe { libc::setxattr(path, name, value, size, 0, flags) } // position=0
}

#[cfg(target_os = "linux")]
pub unsafe fn sys_listxattr(
    path: *const libc::c_char,
    list: *mut libc::c_char,
    size: libc::size_t,
) -> libc::ssize_t {
    libc::listxattr(path, list, size)
}

#[cfg(target_os = "macos")]
pub unsafe fn sys_listxattr(
    path: *const libc::c_char,
    list: *mut libc::c_char,
    size: libc::size_t,
) -> libc::ssize_t {
    unsafe { libc::listxattr(path, list, size, 0) } // options=0
}

#[cfg(target_os = "linux")]
pub unsafe fn sys_removexattr(path: *const libc::c_char, name: *const libc::c_char) -> libc::c_int {
    libc::removexattr(path, name)
}

#[cfg(target_os = "macos")]
pub unsafe fn sys_removexattr(path: *const libc::c_char, name: *const libc::c_char) -> libc::c_int {
    unsafe { libc::removexattr(path, name, 0) } // options=0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_inode() {
        let mode = libc::S_IFREG;
        assert!(is_safe_inode(mode));

        let mode = libc::S_IFDIR;
        assert!(is_safe_inode(mode));

        let mode = libc::S_IFBLK;
        assert!(!is_safe_inode(mode));

        let mode = libc::S_IFCHR;
        assert!(!is_safe_inode(mode));

        let mode = libc::S_IFIFO;
        assert!(!is_safe_inode(mode));

        let mode = libc::S_IFLNK;
        assert!(!is_safe_inode(mode));

        let mode = libc::S_IFSOCK;
        assert!(!is_safe_inode(mode));
    }

    #[test]
    fn test_is_dir() {
        let mode = libc::S_IFREG;
        assert!(!is_dir(mode));

        let mode = libc::S_IFDIR;
        assert!(is_dir(mode));
    }

    #[test]
    fn test_generate_unique_inode() {
        // use normal inode format
        {
            let generator = UniqueInodeGenerator::new();

            let inode_alt_key = InodeId {
                ino: 1,
                dev: 0,
                mnt: 0,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 0
            // 55~48 bit = 0000 0001
            // 47~1 bit  = 1
            assert_eq!(unique_inode, 0x00800000000001);
            let expect_inode_alt_key = generator.decode_unique_inode(unique_inode).unwrap();
            assert_eq!(expect_inode_alt_key, inode_alt_key);

            let inode_alt_key = InodeId {
                ino: 1,
                dev: 0,
                mnt: 1,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 0
            // 55~48 bit = 0000 0010
            // 47~1 bit  = 1
            assert_eq!(unique_inode, 0x01000000000001);
            let expect_inode_alt_key = generator.decode_unique_inode(unique_inode).unwrap();
            assert_eq!(expect_inode_alt_key, inode_alt_key);

            let inode_alt_key = InodeId {
                ino: 2,
                dev: 0,
                mnt: 1,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 0
            // 55~48 bit = 0000 0010
            // 47~1 bit  = 2
            assert_eq!(unique_inode, 0x01000000000002);
            let expect_inode_alt_key = generator.decode_unique_inode(unique_inode).unwrap();
            assert_eq!(expect_inode_alt_key, inode_alt_key);

            let inode_alt_key = InodeId {
                ino: MAX_HOST_INO,
                dev: 0,
                mnt: 1,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 0
            // 55~48 bit = 0000 0010
            // 47~1 bit  = 0x7fffffffffff
            assert_eq!(unique_inode, 0x017fffffffffff);
            let expect_inode_alt_key = generator.decode_unique_inode(unique_inode).unwrap();
            assert_eq!(expect_inode_alt_key, inode_alt_key);
        }

        // use virtual inode format
        {
            let generator = UniqueInodeGenerator::new();
            let inode_alt_key = InodeId {
                ino: MAX_HOST_INO + 1,
                dev: u64::MAX,
                mnt: u64::MAX,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 1
            // 55~48 bit = 0000 0001
            // 47~1 bit  = 2 virtual inode start from 2~MAX_HOST_INO
            assert_eq!(unique_inode, 0x80800000000001);

            let inode_alt_key = InodeId {
                ino: MAX_HOST_INO + 2,
                dev: u64::MAX,
                mnt: u64::MAX,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 1
            // 55~48 bit = 0000 0001
            // 47~1 bit  = 2
            assert_eq!(unique_inode, 0x80800000000002);

            let inode_alt_key = InodeId {
                ino: MAX_HOST_INO + 3,
                dev: u64::MAX,
                mnt: 0,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 1
            // 55~48 bit = 0000 0010
            // 47~1 bit  = 3
            assert_eq!(unique_inode, 0x81000000000003);

            let inode_alt_key = InodeId {
                ino: u64::MAX,
                dev: u64::MAX,
                mnt: u64::MAX,
            };
            let unique_inode = generator.get_unique_inode(&inode_alt_key).unwrap();
            // 56 bit = 1
            // 55~48 bit = 0000 0001
            // 47~1 bit  = 4
            assert_eq!(unique_inode, 0x80800000000004);
        }
    }

    #[test]
    fn test_stat_fd() {
        let topdir = std::env::current_dir().unwrap();
        let dir = File::open(&topdir).unwrap();
        let filename = CString::new("Cargo.toml").unwrap();

        let st1 = stat_fd(&dir, None).unwrap();
        let st2 = stat_fd(&dir, Some(&filename)).unwrap();

        assert_eq!(st1.st_dev, st2.st_dev);
        assert_ne!(st1.st_ino, st2.st_ino);
    }
}
