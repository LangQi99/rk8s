use std::path::{Path, PathBuf};
use std::fs;
use nix::mount::{mount, umount, umount2, MsFlags, MntFlags};
use tracing::{info, warn, error};

#[derive(Debug)]
pub struct BindMount {
    target: PathBuf,
}

impl BindMount {
    pub fn new(source: &Path, target: &Path, read_only: bool) -> Result<Self, String> {
        // Check source type
        let metadata = fs::metadata(source).map_err(|e| format!("Failed to stat source {:?}: {}", source, e))?;

        // Ensure target exists and is of correct type
        if !target.exists() {
            if metadata.is_dir() {
                fs::create_dir_all(target).map_err(|e| format!("Failed to create target dir {:?}: {}", target, e))?;
            } else {
                // Ensure parent dir exists
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent dir {:?}: {}", parent, e))?;
                }
                // Create empty file
                fs::File::create(target).map_err(|e| format!("Failed to create target file {:?}: {}", target, e))?;
            }
        } else {
             // Target exists, check type consistency
             let target_meta = fs::metadata(target).map_err(|e| format!("Failed to stat target {:?}: {}", target, e))?;
             if metadata.is_dir() != target_meta.is_dir() {
                 return Err(format!("Source and target type mismatch: source is_dir={}, target is_dir={}", metadata.is_dir(), target_meta.is_dir()));
             }
        }

        info!("Bind mounting {:?} to {:?} (ro: {})", source, target, read_only);

        // First bind mount
        let flags = MsFlags::MS_BIND; // | MsFlags::MS_REC; // Recursive bind mount? Usually yes for volumes.
        // Let's stick to simple bind first as per test requirements.
        
        mount(Some(source), target, None::<&str>, flags, None::<&str>)
            .map_err(|e| format!("Failed to bind mount {:?} to {:?}: {}", source, target, e))?;

        // If read-only, remount
        if read_only {
            let remount_flags = MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY;
             mount(Some(source), target, None::<&str>, remount_flags, None::<&str>)
                .map_err(|e| format!("Failed to remount read-only {:?}: {}", target, e))?;
        }

        Ok(Self {
            target: target.to_path_buf(),
        })
    }
}

impl Drop for BindMount {
    fn drop(&mut self) {
        info!("Unmounting {:?}", self.target);
        // Try unmounting
        if let Err(e) = umount(&self.target) {
             warn!("Failed to unmount {:?}: {}. Retrying with MNT_DETACH...", self.target, e);
             if let Err(e2) = umount2(&self.target, MntFlags::MNT_DETACH) {
                 error!("Failed to lazy unmount {:?}: {}", self.target, e2);
             }
        }
    }
}

pub struct BindManager {
    mounts: Vec<BindMount>,
}

impl BindManager {
    pub fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    /// Parse bind arguments and perform mounts relative to a base directory
    /// format: target:source[:ro]
    /// target is relative to base_dir
    pub fn mount_all(&mut self, base_dir: &Path, bind_args: &[String]) -> Result<(), String> {
        for arg in bind_args {
            let parts: Vec<&str> = arg.split(':').collect();
            if parts.len() < 2 || parts.len() > 3 {
                return Err(format!("Invalid bind argument format: {}", arg));
            }

            let rel_target = parts[0];
            let source = PathBuf::from(parts[1]);
            let mut read_only = false;
            
            if parts.len() == 3 {
                if parts[2] == "ro" {
                    read_only = true;
                } else {
                    return Err(format!("Invalid bind option: {}", parts[2]));
                }
            }

            // Prevent path traversal
            let target = base_dir.join(rel_target);
            // Simple check to ensure target is inside base_dir
            if !target.starts_with(base_dir) {
                 return Err(format!("Target path {:?} attempts to escape base directory", target));
            }

            let bind_mount = BindMount::new(&source, &target, read_only)?;
            self.mounts.push(bind_mount);
        }
        Ok(())
    }
}

