// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Simple passthrough filesystem example for integration tests.

use clap::Parser;
use libfuse_fs::passthrough::{
    PassthroughArgs, new_passthroughfs_layer, newlogfs::LoggingFileSystem,
};
use rfuse3::{MountOptions, raw::Session};
use std::ffi::OsString;
use std::path::PathBuf;
use tokio::signal;
use tracing::debug;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Passthrough FS example for integration tests"
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
    let args = Args::parse();
    set_log();
    debug!("Starting passthrough filesystem with args: {:?}", args);

    let mut bind_mounts = Vec::new();
    for bind in &args.bind {
        bind_mounts.push((bind.mount_point.clone(), bind.host_path.clone(), bind.readonly));
    }

    let passthrough_args = PassthroughArgs {
        root_dir: &args.rootdir,
        mapping: args.options.as_deref(),
        bind_mounts,
    };

    let fs = new_passthroughfs_layer(passthrough_args)
        .await
        .expect("failed to create passthrough fs");

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
        debug!("Mounting passthrough (unprivileged)");
        Session::new(mount_options)
            .mount_with_unprivileged(fs, mount_path)
            .await
            .expect("Unprivileged mount failed")
    } else {
        debug!("Mounting passthrough (privileged)");
        Session::new(mount_options)
            .mount(fs, mount_path)
            .await
            .expect("Privileged mount failed")
    };

    let handle = &mut mount_handle;
    tokio::select! {
        res = handle => res.unwrap(),
        _ = signal::ctrl_c() => {
            mount_handle.unmount().await.unwrap();
        }
    }
}
