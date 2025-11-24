// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Simple passthrough filesystem example for integration tests.

use clap::Parser;
use libfuse_fs::passthrough::{
    BindMount, Config, PassthroughFs, newlogfs::LoggingFileSystem,
};
use rfuse3::{MountOptions, raw::Session};
use std::ffi::OsString;
use std::path::PathBuf;
use tokio::signal;
use tracing::{debug, info};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Passthrough FS example with bind mount support"
)]
struct Args {
    /// Path to mount point
    #[arg(long)]
    mountpoint: String,
    /// Source directory to expose
    #[arg(long)]
    rootdir: String,
    /// Use privileged mount instead of unprivileged (default false)
    #[arg(long, default_value_t = false)]
    privileged: bool,
    /// Bind mount: mount_point:host_path or mount_point:host_path:ro
    /// Can be specified multiple times.
    /// Example: --bind volumes:/tmp/host --bind data:/tmp/data:ro
    #[arg(long, value_parser = parse_bind_mount)]
    bind: Vec<BindMountArg>,
    /// Options, currently contains uid/gid mapping info
    #[arg(long, short)]
    options: Option<String>,
    #[arg(long)]
    allow_other: bool,
}

#[derive(Debug, Clone)]
struct BindMountArg {
    mount_point: PathBuf,
    host_path: PathBuf,
    readonly: bool,
}

fn parse_bind_mount(s: &str) -> Result<BindMountArg, String> {
    let parts: Vec<&str> = s.split(':').collect();
    
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!(
            "Invalid bind mount format '{}'. Expected: mount_point:host_path[:ro]",
            s
        ));
    }
    
    let mount_point = PathBuf::from(parts[0]);
    let host_path = PathBuf::from(parts[1]);
    let readonly = parts.get(2).map(|&s| s == "ro").unwrap_or(false);
    
    Ok(BindMountArg {
        mount_point,
        host_path,
        readonly,
    })
}

fn set_log() {
    let log_level = "trace";
    let filter_str = format!("libfuse_fs={}", log_level);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter_str));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    let args = Args::parse();
    set_log();
    debug!("Starting passthrough filesystem with args: {:?}", args);

    // Create configuration with mapping support
    let mut config = Config {
        root_dir: args.rootdir.clone().into(),
        do_import: true,
        xattr: true,
        ..Default::default()
    };

    // Parse and apply mapping if provided
    if let Some(ref mapping_str) = args.options {
        config.mapping = mapping_str.parse().expect("Failed to parse mapping");
    }

    // Add bind mounts from arguments
    for bind_arg in &args.bind {
        info!(
            "Configuring bind mount: {:?} -> {:?} (readonly: {})",
            bind_arg.mount_point, bind_arg.host_path, bind_arg.readonly
        );
        
        config.bind_mounts.insert(
            bind_arg.mount_point.clone(),
            BindMount::new(
                bind_arg.mount_point.clone(),
                bind_arg.host_path.clone(),
                bind_arg.readonly,
            ),
        );
    }

    // Create filesystem
    let fs = PassthroughFs::<()>::new(config)
        .expect("Failed to create passthrough fs");
    
    // Import root and initialize bind mounts
    fs.import().await.expect("Failed to import filesystem");
    
    if !args.bind.is_empty() {
        info!("Initialized {} bind mount(s)", args.bind.len());
    }
    
    let fs = LoggingFileSystem::new(fs);

    let fs = LoggingFileSystem::new(fs);
    let mount_path = OsString::from(&args.mountpoint);
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    mount_options
        .force_readdir_plus(true)
        .uid(uid)
        .gid(gid)
        .allow_other(args.allow_other);

    let mut mount_handle = if !args.privileged {
        info!("Mounting passthrough (unprivileged) at {}", args.mountpoint);
        Session::new(mount_options)
            .mount_with_unprivileged(fs, mount_path)
            .await
            .expect("Unprivileged mount failed")
    } else {
        info!("Mounting passthrough (privileged) at {}", args.mountpoint);
        Session::new(mount_options)
            .mount(fs, mount_path)
            .await
            .expect("Privileged mount failed")
    };

    info!("Filesystem mounted successfully. Press Ctrl+C to unmount.");

    let handle = &mut mount_handle;
    tokio::select! {
        res = handle => res.unwrap(),
        _ = signal::ctrl_c() => {
            info!("Unmounting filesystem...");
            mount_handle.unmount().await.unwrap();
            info!("Filesystem unmounted successfully.");
        }
    }
}
