// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Simple passthrough filesystem example for integration tests.

use clap::Parser;
use libfuse_fs::passthrough::{new_passthroughfs_layer, newlogfs::LoggingFileSystem};
use rfuse3::{MountOptions, raw::Session};
use std::ffi::OsString;
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
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let fs = new_passthroughfs_layer(&args.rootdir)
        .await
        .expect("Failed to init passthrough fs");
    let fs = LoggingFileSystem::new(fs);

    let mount_path = OsString::from(&args.mountpoint);
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    // Don't set force_readdir_plus explicitly - use default (like vfs example)
    mount_options.uid(uid).gid(gid);

    // macOS uses unprivileged mount, like vfs example
    #[cfg(target_os = "macos")]
    let mut mount_handle = {
        debug!("Mounting passthrough on macOS (unprivileged)");
        eprintln!("About to call mount_with_unprivileged...");
        match Session::new(mount_options)
            .mount_with_unprivileged(fs, mount_path)
            .await
        {
            Ok(handle) => {
                eprintln!("Mount succeeded!");
                handle
            }
            Err(e) => {
                eprintln!("Mount failed with error: {:?}", e);
                panic!("Mount failed: {:?}", e);
            }
        }
    };

    #[cfg(not(target_os = "macos"))]
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
