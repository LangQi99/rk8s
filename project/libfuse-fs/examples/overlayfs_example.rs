// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Example binary to mount overlay filesystem implemented by libfuse-fs.
// Used by integration tests (fio & IOR) for overlayfs validation.

use clap::Parser;
use libfuse_fs::overlayfs::{OverlayArgs, mount_fs};
use tokio::signal;
use tracing::debug;

#[derive(Parser, Debug)]
#[command(author, version, about = "OverlayFS example for integration tests")]
struct Args {
    /// Mount point path
    #[arg(long)]
    mountpoint: String,
    /// Upper writable layer directory
    #[arg(long)]
    upperdir: String,
    /// Lower read-only layer directories (repeatable)
    #[arg(long)]
    lowerdir: Vec<String>,
    /// Use privileged mount instead of unprivileged (default false)
    #[arg(long, default_value_t = false)]
    privileged: bool,
    /// Options, currently contains uid/gid mapping info
    #[arg(long, short)]
    mapping: Option<String>,
    /// Bind mount: mount_point:host_path or mount_point:host_path:ro
    /// Can be specified multiple times.
    /// Example: --bind volumes:/tmp/host --bind data:/tmp/data:ro
    #[arg(long, value_parser = parse_bind_mount)]
    bind: Vec<BindMountArg>,
    #[arg(long)]
    allow_other: bool,
}

#[derive(Debug, Clone)]
struct BindMountArg {
    mount_point: std::path::PathBuf,
    host_path: std::path::PathBuf,
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
    
    let mount_point = std::path::PathBuf::from(parts[0]);
    let host_path = std::path::PathBuf::from(parts[1]);
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
    debug!("Starting overlay filesystem with args: {:?}", args);

    let bind_mounts = args.bind.iter().map(|b| (b.mount_point.clone(), b.host_path.clone(), b.readonly)).collect();

    let mut mount_handle = mount_fs(OverlayArgs {
        name: None::<String>,
        mountpoint: args.mountpoint,
        lowerdir: args.lowerdir,
        upperdir: args.upperdir,
        mapping: args.mapping,
        privileged: args.privileged,
        allow_other: args.allow_other,
        bind_mounts,
    })
    .await;

    let handle = &mut mount_handle;
    tokio::select! {
        res = handle => res.unwrap(),
        _ = signal::ctrl_c() => {
            mount_handle.unmount().await.unwrap();
        }
    }
}
