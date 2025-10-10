#!/bin/bash

# 调试测试脚本

set -e

echo "=== 调试 rfuse3 示例 ==="

# 清理之前的挂载
echo "清理之前的挂载..."
fusermount -u /tmp/minimal_rfuse3_test 2>/dev/null || true
rmdir /tmp/minimal_rfuse3_test 2>/dev/null || true

# 创建挂载点
echo "创建挂载点..."
mkdir -p /tmp/minimal_rfuse3_test

# 检查 FUSE 设备
echo "检查 FUSE 设备..."
ls -la /dev/macfuse* | head -3

# 检查 macFUSE 安装
echo "检查 macFUSE 安装..."
ls -la /Library/Filesystems/macfuse.fs/Contents/Resources/mount_macfuse

# 构建示例
echo "构建示例..."
cargo build --bin minimal_rfuse3_example

# 运行示例（前台，带详细日志）
echo "运行示例..."
RUST_LOG=debug ./target/debug/minimal_rfuse3_example --mountpoint /tmp/minimal_rfuse3_test
