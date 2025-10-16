use clap::Parser;
use futures_util::Stream;
use rfuse3::{
    raw::{prelude::*, Filesystem, Session},
    MountOptions, Result,
};
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};
use tokio::signal;
use tracing::{debug, info, warn};

/// 最小化的只读文件系统实现
#[derive(Debug)]
struct MinimalFileSystem {
    content: String,
}

impl MinimalFileSystem {
    fn new() -> Self {
        Self {
            content: "Hello, rfuse3! 这是一个最小化的文件系统示例。\n".to_string(),
        }
    }
}

impl Filesystem for MinimalFileSystem {
    async fn init(&self, _req: Request) -> Result<ReplyInit> {
        info!("文件系统初始化");
        Ok(ReplyInit {
            max_write: std::num::NonZeroU32::new(4096).unwrap(),
        })
    }

    async fn destroy(&self, _req: Request) {
        info!("文件系统销毁");
    }

    async fn lookup(&self, _req: Request, parent: u64, name: &OsStr) -> Result<ReplyEntry> {
        let name_str = name.to_string_lossy();
        debug!("查找文件: parent={}, name={}", parent, name_str);

        if parent == 1 && name_str == "hello.txt" {
            let attr = FileAttr {
                ino: 2,
                size: self.content.len() as u64,
                blocks: 1,
                atime: SystemTime::now().into(),
                mtime: SystemTime::now().into(),
                ctime: SystemTime::now().into(),
                #[cfg(target_os = "macos")]
                crtime: SystemTime::now().into(),
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                #[cfg(target_os = "macos")]
                flags: 0,
            };

            Ok(ReplyEntry {
                ttl: Duration::from_secs(1),
                attr,
                generation: 0,
            })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn getattr(
        &self,
        _req: Request,
        inode: u64,
        _fh: Option<u64>,
        _flags: u32,
    ) -> Result<ReplyAttr> {
        debug!("获取属性: inode={}", inode);

        if inode == 1 {
            // 根目录
            let attr = FileAttr {
                ino: 1,
                size: 0,
                blocks: 0,
                atime: SystemTime::now().into(),
                mtime: SystemTime::now().into(),
                ctime: SystemTime::now().into(),
                #[cfg(target_os = "macos")]
                crtime: SystemTime::now().into(),
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                #[cfg(target_os = "macos")]
                flags: 0,
            };
            Ok(ReplyAttr {
                ttl: Duration::from_secs(1),
                attr,
            })
        } else if inode == 2 {
            // hello.txt 文件
            let attr = FileAttr {
                ino: 2,
                size: self.content.len() as u64,
                blocks: 1,
                atime: SystemTime::now().into(),
                mtime: SystemTime::now().into(),
                ctime: SystemTime::now().into(),
                #[cfg(target_os = "macos")]
                crtime: SystemTime::now().into(),
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                #[cfg(target_os = "macos")]
                flags: 0,
            };
            Ok(ReplyAttr {
                ttl: Duration::from_secs(1),
                attr,
            })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn opendir(&self, _req: Request, inode: u64, _flags: u32) -> Result<ReplyOpen> {
        debug!("打开目录: inode={}", inode);

        if inode == 1 {
            // 根目录
            Ok(ReplyOpen {
                fh: 1, // 文件句柄
                flags: 0,
            })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn readdir<'a>(
        &'a self,
        _req: Request,
        parent: u64,
        _fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory<impl Stream<Item = Result<DirectoryEntry>> + Send + 'a>> {
        debug!("读取目录: parent={}, offset={}", parent, offset);

        if parent == 1 {
            // 根目录，根据 offset 返回相应的条目
            let all_entries = vec![
                Ok(DirectoryEntry {
                    inode: 1,
                    offset: 1,
                    kind: FileType::Directory,
                    name: std::ffi::OsString::from("."),
                }),
                Ok(DirectoryEntry {
                    inode: 1,
                    offset: 2,
                    kind: FileType::Directory,
                    name: std::ffi::OsString::from(".."),
                }),
                Ok(DirectoryEntry {
                    inode: 2,
                    offset: 3,
                    kind: FileType::RegularFile,
                    name: std::ffi::OsString::from("hello.txt"),
                }),
            ];

            // 根据 offset 过滤条目
            let filtered_entries: Vec<_> = all_entries
                .into_iter()
                .filter(|entry| {
                    if let Ok(entry) = entry {
                        entry.offset > offset
                    } else {
                        false
                    }
                })
                .collect();

            let stream = futures_util::stream::iter(filtered_entries);
            Ok(ReplyDirectory { entries: stream })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn readdirplus<'a>(
        &'a self,
        _req: Request,
        parent: u64,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus<impl Stream<Item = Result<DirectoryEntryPlus>> + Send + 'a>>
    {
        debug!("读取目录plus: parent={}, offset={}", parent, offset);

        if parent == 1 {
            // 根目录，根据 offset 返回相应的条目及其属性
            let all_entries = vec![
                Ok(DirectoryEntryPlus {
                    inode: 1,
                    generation: 0,
                    kind: FileType::Directory,
                    name: std::ffi::OsString::from("."),
                    offset: 1,
                    attr: FileAttr {
                        ino: 1,
                        size: 0,
                        blocks: 0,
                        atime: SystemTime::now().into(),
                        mtime: SystemTime::now().into(),
                        ctime: SystemTime::now().into(),
                        #[cfg(target_os = "macos")]
                        crtime: SystemTime::now().into(),
                        kind: FileType::Directory,
                        perm: 0o755,
                        nlink: 2,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: 4096,
                        #[cfg(target_os = "macos")]
                        flags: 0,
                    },
                    entry_ttl: Duration::from_secs(1),
                    attr_ttl: Duration::from_secs(1),
                }),
                Ok(DirectoryEntryPlus {
                    inode: 1,
                    generation: 0,
                    kind: FileType::Directory,
                    name: std::ffi::OsString::from(".."),
                    offset: 2,
                    attr: FileAttr {
                        ino: 1,
                        size: 0,
                        blocks: 0,
                        atime: SystemTime::now().into(),
                        mtime: SystemTime::now().into(),
                        ctime: SystemTime::now().into(),
                        #[cfg(target_os = "macos")]
                        crtime: SystemTime::now().into(),
                        kind: FileType::Directory,
                        perm: 0o755,
                        nlink: 2,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: 4096,
                        #[cfg(target_os = "macos")]
                        flags: 0,
                    },
                    entry_ttl: Duration::from_secs(1),
                    attr_ttl: Duration::from_secs(1),
                }),
                Ok(DirectoryEntryPlus {
                    inode: 2,
                    generation: 0,
                    kind: FileType::RegularFile,
                    name: std::ffi::OsString::from("hello.txt"),
                    offset: 3,
                    attr: FileAttr {
                        ino: 2,
                        size: self.content.len() as u64,
                        blocks: 1,
                        atime: SystemTime::now().into(),
                        mtime: SystemTime::now().into(),
                        ctime: SystemTime::now().into(),
                        #[cfg(target_os = "macos")]
                        crtime: SystemTime::now().into(),
                        kind: FileType::RegularFile,
                        perm: 0o644,
                        nlink: 1,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: 4096,
                        #[cfg(target_os = "macos")]
                        flags: 0,
                    },
                    entry_ttl: Duration::from_secs(1),
                    attr_ttl: Duration::from_secs(1),
                }),
            ];

            // 根据 offset 过滤条目
            let filtered_entries: Vec<_> = all_entries
                .into_iter()
                .filter(|entry| {
                    if let Ok(entry) = entry {
                        entry.offset > offset as i64
                    } else {
                        false
                    }
                })
                .collect();

            let stream = futures_util::stream::iter(filtered_entries);
            Ok(ReplyDirectoryPlus { entries: stream })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn open(&self, _req: Request, inode: u64, flags: u32) -> Result<ReplyOpen> {
        debug!("打开文件: inode={}, flags={}", inode, flags);

        if inode == 2 {
            Ok(ReplyOpen { fh: 2, flags: 0 })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn read(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        debug!(
            "读取文件: inode={}, offset={}, size={}",
            inode, offset, size
        );

        if inode == 2 {
            let start = offset as usize;
            let end = std::cmp::min(start + size as usize, self.content.len());

            if start < self.content.len() {
                let data = self.content[start..end].as_bytes().to_vec();
                Ok(ReplyData { data: data.into() })
            } else {
                Ok(ReplyData {
                    data: Vec::new().into(),
                })
            }
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn statfs(&self, _req: Request, _inode: u64) -> Result<ReplyStatFs> {
        debug!("获取文件系统统计信息");

        Ok(ReplyStatFs {
            blocks: 1000, // 总块数
            bfree: 800,   // 空闲块数
            bavail: 800,  // 可用块数
            files: 100,   // 总文件数
            ffree: 50,    // 空闲文件数
            bsize: 4096,  // 块大小
            namelen: 255, // 最大文件名长度
            frsize: 4096, // 片段大小
        })
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about = "最小化的 rfuse3 文件系统示例")]
struct Args {
    /// 挂载点路径
    #[arg(long)]
    mountpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志 - 设置为debug级别
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();

    // 创建最小化文件系统
    let fs = MinimalFileSystem::new();

    // 配置挂载选项
    let mut mount_options = MountOptions::default();
    mount_options.force_readdir_plus(true);

    // 获取当前用户 ID
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    mount_options.uid(uid).gid(gid);

    let mount_path = std::ffi::OsString::from(&args.mountpoint);

    info!("开始挂载最小化文件系统到: {}", args.mountpoint);

    // 挂载文件系统 - 根据平台和特性选择挂载方式
    let mut mount_handle = {
        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        {
            // Linux 下使用非特权挂载
            Session::new(mount_options)
                .mount_with_unprivileged(fs, mount_path)
                .await
        }
        #[cfg(target_os = "macos")]
        {
            // macOS 下使用非特权挂载
            Session::new(mount_options)
                .mount_with_unprivileged(fs, mount_path)
                .await
        }
        #[cfg(target_os = "freebsd")]
        {
            // FreeBSD 下使用非特权挂载
            Session::new(mount_options)
                .mount_with_unprivileged(fs, mount_path)
                .await
        }
        #[cfg(not(any(
            all(target_os = "linux", feature = "unprivileged"),
            target_os = "macos",
            target_os = "freebsd"
        )))]
        {
            // 其他情况使用普通挂载
            Session::new(mount_options).mount(fs, mount_path).await
        }
    }
    .map_err(|e| {
        eprintln!("挂载失败: {}", e);
        e
    })?;

    info!("文件系统已成功挂载！");
    info!("您可以尝试以下操作：");
    info!("  - ls {}  # 列出目录内容", args.mountpoint);
    info!("  - cat {}/hello.txt  # 读取文件", args.mountpoint);
    info!("按 Ctrl+C 卸载文件系统");

    // 运行文件系统直到收到信号
    let handle = &mut mount_handle;
    tokio::select! {
        res = handle => {
            match res {
                Ok(_) => info!("文件系统正常退出"),
                Err(e) => {
                    warn!("文件系统运行出错: {}", e);
                    return Err(e.into());
                }
            }
        },
        _ = signal::ctrl_c() => {
            info!("收到退出信号，正在卸载文件系统...");
            mount_handle.unmount().await.map_err(|e| {
                eprintln!("卸载失败: {}", e);
                e
            })?;
            info!("文件系统已卸载");
        }
    }

    Ok(())
}
