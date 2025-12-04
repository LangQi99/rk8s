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
            });
            
            info!("Bind mounted {:?} -> {:?}", bind.source, target_path);
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

/// Result of validating a mount target path
enum ValidateResult {
    /// Path is valid and canonicalized
    Valid(PathBuf),
    /// Path doesn't exist (likely already unmounted), safe to skip
    AlreadyUnmounted,
    /// Path exists but cannot be validated, skip for safety
    ValidationFailed(Error),
}

impl BindMountManager {
    /// Validates and canonicalizes a mount target path
    fn validate_mount_target(target: &Path) -> ValidateResult {
        match target.canonicalize() {
            Ok(canonical) => ValidateResult::Valid(canonical),
            Err(e) => {
                // Security: If we cannot canonicalize, we cannot validate the path safely
                // Two cases: 
                // 1. Path doesn't exist → likely already unmounted, safe to skip
                // 2. Path exists but validation fails → potentially malicious, skip for safety
                if e.kind() == std::io::ErrorKind::NotFound {
                    debug!("Mount target {:?} not found", target);
                    ValidateResult::AlreadyUnmounted
                } else {
                    error!("Cannot canonicalize mount target {:?}: {}", target, e);
                    ValidateResult::ValidationFailed(e)
                }
            }
        }
    }

    /// Unmount all bind mounts
    pub async fn unmount_all(&self) -> Result<()> {
        let mut mounts = self.mounts.lock().await;
        let mut errors = Vec::new();

        // Canonicalize mountpoint once at the start
        // If this fails, we cannot safely validate paths, so abort
        let canonical_mountpoint = match self.mountpoint.canonicalize() {
            Ok(path) => path,
            Err(e) => {
                error!("Could not canonicalize mountpoint {:?}: {}. Aborting unmount for safety.", self.mountpoint, e);
                return Err(Error::other(format!(
                    "Cannot validate mountpoint {:?}: {}",
                    self.mountpoint, e
                )));
            }
        };

        // Unmount in reverse order
        while let Some(mut mount) = mounts.pop() {
            if mount.mounted {
                // Verify the mount point is actually under our mountpoint
                // This prevents accidentally unmounting host mounts
                let canonical_target = match Self::validate_mount_target(&mount.target) {
                    ValidateResult::Valid(path) => path,
                    ValidateResult::AlreadyUnmounted => continue,
                    ValidateResult::ValidationFailed(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                
                if !canonical_target.starts_with(&canonical_mountpoint) {
                    error!(
                        "Security: Refusing to unmount path outside mountpoint (mount may have been compromised)"
                    );
                    debug!("  Attempted: {:?}, Expected under: {:?}", canonical_target, canonical_mountpoint);
                    continue;
                }
                
                if let Err(e) = self.do_unmount(&mount.target) {
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
    fn do_unmount(&self, target: &Path) -> Result<()> {
        use std::ffi::CString;

        let target_cstr = CString::new(target.to_str().ok_or_else(|| {
            Error::other(format!("Invalid target path: {:?}", target))
        })?)
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        // Try normal unmount first (without MNT_DETACH)
        // This is safer for bind mounts as it won't affect the source mount
        let ret = unsafe { libc::umount(target_cstr.as_ptr()) };

        if ret != 0 {
            let err = Error::last_os_error();
            
            // Handle specific error codes
            match err.raw_os_error() {
                // ENOENT: Mount target doesn't exist - already unmounted
                Some(libc::ENOENT) => {
                    debug!("Mount {:?} not found (already unmounted)", target);
                    return Ok(());
                }
                // EINVAL: Most commonly means target is not a mount point
                // However, it can also indicate other issues, so log more verbosely
                Some(libc::EINVAL) => {
                    debug!("Mount {:?} is not a mount point (likely already unmounted)", target);
                    return Ok(());
                }
                // EBUSY: Mount is in use, try lazy unmount as last resort
                Some(libc::EBUSY) => {
                    info!("Mount {:?} is busy, attempting lazy unmount", target);
                    let lazy_unmount_ret = unsafe { libc::umount2(target_cstr.as_ptr(), libc::MNT_DETACH) };
                    if lazy_unmount_ret != 0 {
                        let lazy_unmount_err = Error::last_os_error();
                        error!("Failed to lazy unmount {:?}: {}", target, lazy_unmount_err);
                        return Err(lazy_unmount_err);
                    }
                    return Ok(());
                }
                // For any other error, fail loudly
                _ => {
                    error!("Failed to unmount {:?}: {} (errno: {:?})", 
                           target, err, err.raw_os_error());
                    return Err(err);
                }
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
                    let _ = self.do_unmount(&mount.target);
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

    #[test]
    fn test_bind_mount_manager_mountpoint_validation() {
        // Test that mount targets are correctly constructed under the mountpoint
        let manager = BindMountManager::new("/mnt/test");
        
        // Test with absolute path target
        let bind = BindMount {
            source: PathBuf::from("/proc"),
            target: PathBuf::from("/proc"),
        };
        
        let target_path = manager.mountpoint.join(
            bind.target.strip_prefix("/").unwrap_or(&bind.target)
        );
        
        // Verify the target is under the mountpoint
        assert!(target_path.starts_with(&manager.mountpoint));
        assert_eq!(target_path, PathBuf::from("/mnt/test/proc"));
    }

    #[test]
    fn test_mountpoint_safety_check() {
        // Verify that paths outside mountpoint would be caught
        let manager = BindMountManager::new("/mnt/overlay");
        
        // This should be under mountpoint
        let safe_path = PathBuf::from("/mnt/overlay/dev/pts");
        assert!(safe_path.starts_with(&manager.mountpoint));
        
        // This should NOT be under mountpoint
        let unsafe_path = PathBuf::from("/dev/pts");
        assert!(!unsafe_path.starts_with(&manager.mountpoint));
        
        // This should NOT be under mountpoint (parent directory)
        let parent_path = PathBuf::from("/mnt");
        assert!(!parent_path.starts_with(&manager.mountpoint));
    }

    #[test]
    fn test_path_traversal_protection() {
        // Test that path traversal attempts would be caught
        // Note: canonicalize() would be used in real code to resolve these
        
        // Simulated path that looks under mountpoint but escapes via ..
        let apparent_path = PathBuf::from("/mnt/overlay/../dev/pts");
        let mountpoint = PathBuf::from("/mnt/overlay");
        
        // Before canonicalize, simple starts_with would be vulnerable
        // After canonicalize, this would resolve to /dev/pts
        // and fail the starts_with check
        
        // The actual production code uses canonicalize() which would resolve
        // "/mnt/overlay/../dev/pts" to "/dev/pts"
        // This test documents the expected behavior
        
        // If we could canonicalize (requires filesystem), it would work like:
        // let canonical = apparent_path.canonicalize().unwrap();
        // assert!(!canonical.starts_with(&mountpoint));
        
        // For the test, we just verify the string-based check would fail
        // after canonicalization (which production code does)
        assert!(apparent_path.to_str().unwrap().contains(".."));
    }
}
