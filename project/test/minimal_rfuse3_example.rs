// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// 最小化的 rfuse3 示例 - 基于 libfuse-fs 的工作代码

use clap::Parser;
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
    type DirEntryStream<'a> = futures_util::stream::Iter<std::vec::IntoIter<Result<DirectoryEntry>>>;
    type DirEntryPlusStream<'a> = futures_util::stream::Iter<std::vec::IntoIter<Result<DirectoryEntryPlus>>>;

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
                crtime: SystemTime::now().into(),
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
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

    async fn getattr(&self, _req: Request, inode: u64, _fh: Option<u64>, _flags: u32) -> Result<ReplyAttr> {
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
                crtime: SystemTime::now().into(),
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
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
                crtime: SystemTime::now().into(),
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
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
        _offset: i64,
    ) -> Result<ReplyDirectory<Self::DirEntryStream<'a>>> {
        debug!("读取目录: parent={}", parent);

        if parent == 1 {
            // 根目录，返回 hello.txt
            let entries = vec![
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

            let stream = futures_util::stream::iter(entries);
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
        _offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus<Self::DirEntryPlusStream<'a>>> {
        debug!("读取目录plus: parent={}", parent);

        if parent == 1 {
            // 根目录，返回 hello.txt 及其属性
            let entries = vec![
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
                        crtime: SystemTime::now().into(),
                        kind: FileType::Directory,
                        perm: 0o755,
                        nlink: 2,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: 4096,
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
                        crtime: SystemTime::now().into(),
                        kind: FileType::Directory,
                        perm: 0o755,
                        nlink: 2,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: 4096,
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
                        crtime: SystemTime::now().into(),
                        kind: FileType::RegularFile,
                        perm: 0o644,
                        nlink: 1,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: 4096,
                        flags: 0,
                    },
                    entry_ttl: Duration::from_secs(1),
                    attr_ttl: Duration::from_secs(1),
                }),
            ];

            let stream = futures_util::stream::iter(entries);
            Ok(ReplyDirectoryPlus { entries: stream })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn open(&self, _req: Request, inode: u64, flags: u32) -> Result<ReplyOpen> {
        debug!("打开文件: inode={}, flags={}", inode, flags);

        if inode == 2 {
            Ok(ReplyOpen {
                fh: 2,
                flags: 0,
            })
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
        debug!("读取文件: inode={}, offset={}, size={}", inode, offset, size);

        if inode == 2 {
            let start = offset as usize;
            let end = std::cmp::min(start + size as usize, self.content.len());
            
            if start < self.content.len() {
                let data = self.content[start..end].as_bytes().to_vec();
                Ok(ReplyData { data: data.into() })
            } else {
                Ok(ReplyData { data: Vec::new().into() })
            }
        } else {
            Err(libc::ENOENT.into())
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "最小化的 rfuse3 文件系统示例"
)]
struct Args {
    /// 挂载点路径
    #[arg(long)]
    mountpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

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

    // 挂载文件系统
    // 在 macOS 上，mount() 内部直接调用 mount_with_unprivileged()
    let mut mount_handle = Session::new(mount_options)
        .mount(fs, mount_path)
        .await
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
