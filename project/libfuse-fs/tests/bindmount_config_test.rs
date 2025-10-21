/// Integration tests for bind mount functionality

use libfuse_fs::passthrough::{BindMount, Config, PassthroughFs};
use std::fs;
use std::path::PathBuf;
use vmm_sys_util::tempdir::TempDir;

#[test]
fn test_bind_mount_config_structure() {
    // Test BindMount structure creation
    let bind_mount = BindMount::new(
        PathBuf::from("volumes"),
        PathBuf::from("/host/path"),
        true,
    );
    
    assert_eq!(bind_mount.mount_point, PathBuf::from("volumes"));
    assert_eq!(bind_mount.host_path, PathBuf::from("/host/path"));
    assert_eq!(bind_mount.readonly, true);
    
    println!("✓ BindMount structure works correctly");
}

#[tokio::test]
async fn test_filesystem_creation_with_empty_config() {
    let source_dir = TempDir::new().unwrap();
    
    let config = Config {
        root_dir: source_dir.as_path().to_str().unwrap().to_string(),
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    
    let fs = PassthroughFs::<()>::new(config);
    assert!(fs.is_ok(), "Failed to create filesystem");
    
    let fs = fs.unwrap();
    let result = fs.import().await;
    assert!(result.is_ok(), "Failed to import filesystem");
    
    println!("✓ Filesystem created successfully");
}

#[tokio::test]
async fn test_bind_mount_with_valid_config() {
    let source_dir = TempDir::new().unwrap();
    let host_dir = TempDir::new().unwrap();
    
    // No need to create mount point - it will be auto-created!
    
    // Create test file in host directory
    let test_file = host_dir.as_path().join("test.txt");
    fs::write(&test_file, "Hello from host").unwrap();
    
    // Create filesystem with bind mount in config
    let mut config = Config {
        root_dir: source_dir.as_path().to_str().unwrap().to_string(),
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    
    config.bind_mounts.insert(
        PathBuf::from("volumes"),
        BindMount::new(
            PathBuf::from("volumes"),
            host_dir.as_path().to_path_buf(),
            false,
        ),
    );
    
    let fs = PassthroughFs::<()>::new(config);
    assert!(fs.is_ok(), "Failed to create filesystem with bind mount config");
    
    let fs = fs.unwrap();
    let result = fs.import().await;
    assert!(result.is_ok(), "Failed to import with bind mount: {:?}", result.err());
    
    println!("✓ Filesystem initialized with bind mount from config");
}

#[tokio::test]
async fn test_bind_mount_nonexistent_host_path() {
    let source_dir = TempDir::new().unwrap();
    
    // No need to create mount point directory
    
    // Try to mount non-existent host path
    let mut config = Config {
        root_dir: source_dir.as_path().to_str().unwrap().to_string(),
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    
    config.bind_mounts.insert(
        PathBuf::from("volumes"),
        BindMount::new(
            PathBuf::from("volumes"),
            PathBuf::from("/nonexistent/path/that/does/not/exist"),
            false,
        ),
    );
    
    let fs = PassthroughFs::<()>::new(config);
    assert!(fs.is_ok(), "Failed to create filesystem");
    
    let result = fs.unwrap().import().await;
    assert!(result.is_err(), "Should fail when host path doesn't exist");
    
    println!("✓ Correctly rejected bind mount to non-existent host path");
}


#[tokio::test]
async fn test_multiple_bind_mounts_in_config() {
    let source_dir = TempDir::new().unwrap();
    let host_dir1 = TempDir::new().unwrap();
    let host_dir2 = TempDir::new().unwrap();
    
    // No need to create mount point directories - auto-created!
    
    // Create filesystem with multiple bind mounts
    let mut config = Config {
        root_dir: source_dir.as_path().to_str().unwrap().to_string(),
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    
    config.bind_mounts.insert(
        PathBuf::from("volumes"),
        BindMount::new(
            PathBuf::from("volumes"),
            host_dir1.as_path().to_path_buf(),
            false,
        ),
    );
    
    config.bind_mounts.insert(
        PathBuf::from("data"),
        BindMount::new(
            PathBuf::from("data"),
            host_dir2.as_path().to_path_buf(),
            true, // readonly
        ),
    );
    
    let fs = PassthroughFs::<()>::new(config);
    assert!(fs.is_ok(), "Failed to create filesystem with multiple bind mounts");
    
    let result = fs.unwrap().import().await;
    assert!(result.is_ok(), "Failed to import with multiple bind mounts: {:?}", result.err());
    
    println!("✓ Multiple bind mounts configured successfully");
}

#[tokio::test]
async fn test_readonly_bind_mount() {
    let source_dir = TempDir::new().unwrap();
    let host_dir = TempDir::new().unwrap();
    
    // No need to pre-create directories
    
    let mut config = Config {
        root_dir: source_dir.as_path().to_str().unwrap().to_string(),
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    
    config.bind_mounts.insert(
        PathBuf::from("volumes"),
        BindMount::new(
            PathBuf::from("volumes"),
            host_dir.as_path().to_path_buf(),
            true, // readonly
        ),
    );
    
    let fs = PassthroughFs::<()>::new(config);
    assert!(fs.is_ok());
    
    let result = fs.unwrap().import().await;
    assert!(result.is_ok(), "Failed to create readonly bind mount: {:?}", result.err());
    
    println!("✓ Readonly bind mount created successfully");
}

#[tokio::test]
async fn test_bind_mount_preserves_config() {
    let source_dir = TempDir::new().unwrap();
    let host_dir = TempDir::new().unwrap();
    
    let volumes_dir = source_dir.as_path().join("volumes");
    fs::create_dir_all(&volumes_dir).unwrap();
    
    let host_path = host_dir.as_path().to_path_buf();
    
    let mut config = Config {
        root_dir: source_dir.as_path().to_str().unwrap().to_string(),
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    
    config.bind_mounts.insert(
        PathBuf::from("volumes"),
        BindMount::new(
            PathBuf::from("volumes"),
            host_path.clone(),
            true,
        ),
    );
    
    // Verify bind mount is in config
    assert_eq!(config.bind_mounts.len(), 1);
    let bind_mount = config.bind_mounts.get(&PathBuf::from("volumes")).unwrap();
    assert_eq!(bind_mount.host_path, host_path);
    assert_eq!(bind_mount.readonly, true);
    
    println!("✓ Bind mount configuration preserved correctly");
}

