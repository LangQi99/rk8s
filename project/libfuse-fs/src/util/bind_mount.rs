// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bind mount utilities for container volume management

use std::io::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

/// Represents a single bind mount
#[derive(Debug, Clone)]
pub struct BindMount {
    /// Source path on host
    pub source: PathBuf,
    /// Target path relative to mount point
    pub target: PathBuf,
}

impl BindMount {
    /// Parse a bind mount specification like "proc:/proc" or "/host/path:/container/path"
    pub fn parse(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() != 2 {
            return Err(Error::other(format!(
                "Invalid bind mount spec: '{}'. Expected format: 'source:target'",
                spec
            )));
        }

        let source = PathBuf::from(parts[0]);
        let target = PathBuf::from(parts[1]);

        // Convert relative source paths to absolute from root
        let source = if source.is_relative() {
            PathBuf::from("/").join(source)
        } else {
            source
        };

        Ok(BindMount { source, target })
    }
}

/// Manages multiple bind mounts with automatic cleanup
pub struct BindMountManager {
    mounts: Arc<Mutex<Vec<MountPoint>>>,
    mountpoint: PathBuf,
}

#[derive(Debug)]
struct MountPoint {
    target: PathBuf,
    mounted: bool,
    mount_type: MountType,
    ptmx_link: Option<PathBuf>, // Track ptmx symlink if created
}

#[derive(Debug, Clone, Copy)]
enum MountType {
    Bind,
    Devpts,
}

impl BindMountManager {
    /// Create a new bind mount manager
    pub fn new<P: AsRef<Path>>(mountpoint: P) -> Self {
        Self {
            mounts: Arc::new(Mutex::new(Vec::new())),
            mountpoint: mountpoint.as_ref().to_path_buf(),
        }
    }

    /// Mount all bind mounts
    pub async fn mount_all(&self, bind_specs: &[BindMount]) -> Result<()> {
        let mut mounts = self.mounts.lock().await;

        for bind in bind_specs {
            let target_path = self.mountpoint.join(bind.target.strip_prefix("/").unwrap_or(&bind.target));
            
            // Special handling for /dev/pts - mount fresh devpts instead of bind mount
            let is_devpts = bind.source == PathBuf::from("/dev/pts") 
                || bind.target == PathBuf::from("/dev/pts")
                || bind.target == PathBuf::from("dev/pts");
            
            if is_devpts {
                // Create target directory if it doesn't exist
                if !target_path.exists() {
                    std::fs::create_dir_all(&target_path)?;
                    debug!("Created devpts target directory: {:?}", target_path);
                }
                
                // Mount fresh devpts filesystem
                self.do_mount_devpts(&target_path)?;
                
                // Create /dev/ptmx symlink if it doesn't exist
                let ptmx_link = if let Some(dev_path) = target_path.parent() {
                    let ptmx_path = dev_path.join("ptmx");
                    // Only create symlink if ptmx doesn't exist or is not a device node
                    if !ptmx_path.exists() || !ptmx_path.is_symlink() {
                        // Create symlink: /dev/ptmx -> /dev/pts/ptmx
                        if ptmx_path.exists() {
                            // Remove existing file/link if it exists
                            let _ = std::fs::remove_file(&ptmx_path);
                        }
                        std::os::unix::fs::symlink("pts/ptmx", &ptmx_path)?;
                        debug!("Created ptmx symlink at {:?}", ptmx_path);
                        Some(ptmx_path)
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                mounts.push(MountPoint {
                    target: target_path.clone(),
                    mounted: true,
                    mount_type: MountType::Devpts,
                    ptmx_link,
                });
                
                info!("Mounted devpts at {:?}", target_path);
            } else {
                // Check if source is a file or directory
                let source_metadata = std::fs::metadata(&bind.source)?;
                
                if !target_path.exists() {
                    if source_metadata.is_file() {
                        // For file bind mounts, create parent directory and an empty file
                        if let Some(parent) = target_path.parent() {
                            std::fs::create_dir_all(parent)?;
                            debug!("Created parent directory: {:?}", parent);
                        }
                        std::fs::File::create(&target_path)?;
                        debug!("Created target file: {:?}", target_path);
                    } else {
                        // For directory bind mounts, create the directory
                        std::fs::create_dir_all(&target_path)?;
                        debug!("Created target directory: {:?}", target_path);
                    }
                }

                // Perform the bind mount
                self.do_mount(&bind.source, &target_path)?;
                
                mounts.push(MountPoint {
                    target: target_path.clone(),
                    mounted: true,
                    mount_type: MountType::Bind,
                    ptmx_link: None,
                });
                
                info!("Bind mounted {:?} -> {:?}", bind.source, target_path);
            }
        }

        Ok(())
    }

    /// Perform the actual bind mount using mount(2) syscall
    fn do_mount(&self, source: &Path, target: &Path) -> Result<()> {
        use std::ffi::CString;

        let source_cstr = CString::new(source.to_str().ok_or_else(|| {
            Error::other(format!("Invalid source path: {:?}", source))
        })?)
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        let target_cstr = CString::new(target.to_str().ok_or_else(|| {
            Error::other(format!("Invalid target path: {:?}", target))
        })?)
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        let fstype = CString::new("none").unwrap();

        let ret = unsafe {
            libc::mount(
                source_cstr.as_ptr(),
                target_cstr.as_ptr(),
                fstype.as_ptr(),
                libc::MS_BIND | libc::MS_REC,
                std::ptr::null(),
            )
        };

        if ret != 0 {
            let err = Error::last_os_error();
            error!("Failed to bind mount {:?} to {:?}: {}", source, target, err);
            return Err(err);
        }

        Ok(())
    }

    /// Mount a fresh devpts filesystem
    fn do_mount_devpts(&self, target: &Path) -> Result<()> {
        use std::ffi::CString;

        let target_cstr = CString::new(target.to_str().ok_or_else(|| {
            Error::other(format!("Invalid target path: {:?}", target))
        })?)
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        let fstype = CString::new("devpts").unwrap();
        let options = CString::new("newinstance,ptmxmode=0666,mode=0620").unwrap();

        let ret = unsafe {
            libc::mount(
                std::ptr::null(),
                target_cstr.as_ptr(),
                fstype.as_ptr(),
                0, // No special flags needed for devpts
                options.as_ptr() as *const libc::c_void,
            )
        };

        if ret != 0 {
            let err = Error::last_os_error();
            error!("Failed to mount devpts at {:?}: {}", target, err);
            return Err(err);
        }

        Ok(())
    }

    /// Unmount all bind mounts
    pub async fn unmount_all(&self) -> Result<()> {
        let mut mounts = self.mounts.lock().await;
        let mut errors = Vec::new();

        // Unmount in reverse order
        while let Some(mut mount) = mounts.pop() {
            if mount.mounted {
                // Remove ptmx symlink if we created it
                if let Some(ref ptmx_link) = mount.ptmx_link {
                    if ptmx_link.is_symlink() {
                        if let Err(e) = std::fs::remove_file(ptmx_link) {
                            error!("Failed to remove ptmx symlink {:?}: {}", ptmx_link, e);
                        } else {
                            debug!("Removed ptmx symlink {:?}", ptmx_link);
                        }
                    }
                }
                
                if let Err(e) = self.do_unmount(&mount.target, mount.mount_type) {
                    error!("Failed to unmount {:?}: {}", mount.target, e);
                    errors.push(e);
                } else {
                    mount.mounted = false;
                    info!("Unmounted {:?}", mount.target);
                }
            }
        }

        if !errors.is_empty() {
            return Err(Error::other(format!(
                "Failed to unmount {} bind mounts",
                errors.len()
            )));
        }

        Ok(())
    }

    /// Perform the actual unmount using umount(2) syscall
    fn do_unmount(&self, target: &Path, mount_type: MountType) -> Result<()> {
        use std::ffi::CString;

        let target_cstr = CString::new(target.to_str().ok_or_else(|| {
            Error::other(format!("Invalid target path: {:?}", target))
        })?)
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        // For devpts mounts, use regular unmount (not lazy)
        // For bind mounts, use MNT_DETACH for more reliable cleanup
        let flags = match mount_type {
            MountType::Devpts => 0, // Regular unmount
            MountType::Bind => libc::MNT_DETACH, // Lazy unmount
        };

        let ret = unsafe { libc::umount2(target_cstr.as_ptr(), flags) };

        if ret != 0 {
            let err = Error::last_os_error();
            // EINVAL or ENOENT might mean it's already unmounted
            if err.raw_os_error() != Some(libc::EINVAL)
                && err.raw_os_error() != Some(libc::ENOENT)
            {
                return Err(err);
            }
        }

        Ok(())
    }
}

impl Drop for BindMountManager {
    fn drop(&mut self) {
        // Attempt to clean up on drop (synchronously)
        let mounts = self.mounts.try_lock();
        if let Ok(mut mounts) = mounts {
            while let Some(mount) = mounts.pop() {
                if mount.mounted {
                    // Remove ptmx symlink if we created it
                    if let Some(ref ptmx_link) = mount.ptmx_link {
                        if ptmx_link.is_symlink() {
                            let _ = std::fs::remove_file(ptmx_link);
                        }
                    }
                    let _ = self.do_unmount(&mount.target, mount.mount_type);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bind_mount() {
        let bind = BindMount::parse("proc:/proc").unwrap();
        assert_eq!(bind.source, PathBuf::from("/proc"));
        assert_eq!(bind.target, PathBuf::from("/proc"));

        let bind = BindMount::parse("/host/path:/container/path").unwrap();
        assert_eq!(bind.source, PathBuf::from("/host/path"));
        assert_eq!(bind.target, PathBuf::from("/container/path"));

        let bind = BindMount::parse("sys:/sys").unwrap();
        assert_eq!(bind.source, PathBuf::from("/sys"));
        assert_eq!(bind.target, PathBuf::from("/sys"));
    }

    #[test]
    fn test_invalid_bind_mount() {
        assert!(BindMount::parse("invalid").is_err());
        assert!(BindMount::parse("too:many:colons").is_err());
    }
}
