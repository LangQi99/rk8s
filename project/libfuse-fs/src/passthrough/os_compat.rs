// Copyright (C) 2020-2022 Alibaba Cloud. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE-BSD-3-Clause file.
// SPDX-License-Identifier: Apache-2.0

use vm_memory::ByteValued;

// Platform-specific type aliases for 64-bit types
// macOS doesn't have separate 64-bit types as it's 64-bit by default
#[cfg(target_os = "macos")]
pub type stat64 = libc::stat;
#[cfg(target_os = "macos")]
pub type off64_t = libc::off_t;
#[cfg(target_os = "macos")]
pub type ino64_t = libc::ino_t;
#[cfg(target_os = "macos")]
pub type statvfs64 = libc::statvfs;

// Linux uses separate 64-bit types
#[cfg(target_os = "linux")]
pub type stat64 = libc::stat64;
#[cfg(target_os = "linux")]
pub type off64_t = libc::off64_t;
#[cfg(target_os = "linux")]
pub type ino64_t = libc::ino64_t;
#[cfg(target_os = "linux")]
pub type statvfs64 = libc::statvfs64;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxDirent64 {
    pub d_ino: ino64_t,
    pub d_off: off64_t,
    pub d_reclen: libc::c_ushort,
    pub d_ty: libc::c_uchar,
}
unsafe impl ByteValued for LinuxDirent64 {}

#[cfg(target_env = "gnu")]
pub use libc::statx as statx_st;

#[cfg(target_env = "gnu")]
pub use libc::{STATX_BASIC_STATS, STATX_MNT_ID};

// musl provides the 'struct statx', but without stx_mnt_id.
// However, the libc crate does not provide libc::statx
// if musl is used. So we add just the required struct and
// constants to make it works.
#[cfg(not(target_env = "gnu"))]
#[repr(C)]
pub struct statx_st_timestamp {
    pub tv_sec: i64,
    pub tv_nsec: u32,
    pub __statx_timestamp_pad1: [i32; 1],
}

#[cfg(not(target_env = "gnu"))]
#[repr(C)]
pub struct statx_st {
    pub stx_mask: u32,
    pub stx_blksize: u32,
    pub stx_attributes: u64,
    pub stx_nlink: u32,
    pub stx_uid: u32,
    pub stx_gid: u32,
    pub stx_mode: u16,
    __statx_pad1: [u16; 1],
    pub stx_ino: u64,
    pub stx_size: u64,
    pub stx_blocks: u64,
    pub stx_attributes_mask: u64,
    pub stx_atime: statx_st_timestamp,
    pub stx_btime: statx_st_timestamp,
    pub stx_ctime: statx_st_timestamp,
    pub stx_mtime: statx_st_timestamp,
    pub stx_rdev_major: u32,
    pub stx_rdev_minor: u32,
    pub stx_dev_major: u32,
    pub stx_dev_minor: u32,
    pub stx_mnt_id: u64,
    __statx_pad2: u64,
    __statx_pad3: [u64; 12],
}

#[cfg(not(target_env = "gnu"))]
pub const STATX_BASIC_STATS: libc::c_uint = 0x07ff;

#[cfg(not(target_env = "gnu"))]
pub const STATX_MNT_ID: libc::c_uint = 0x1000;
