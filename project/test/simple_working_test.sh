#!/bin/bash

# 简单的 rfuse3 测试脚本 - 基于成功的测试

set -e

echo "=== rfuse3 简单测试 ==="

# 清理
echo "清理环境..."
pkill -f minimal_rfuse3_example 2>/dev/null || true
fusermount -u /tmp/minimal_rfuse3_test 2>/dev/null || true
rmdir /tmp/minimal_rfuse3_test 2>/dev/null || true

# 构建
echo "构建示例..."
cargo build --bin minimal_rfuse3_example

# 创建挂载点
echo "创建挂载点..."
mkdir -p /tmp/minimal_rfuse3_test

# 启动文件系统
echo "启动文件系统..."
./target/debug/minimal_rfuse3_example --mountpoint /tmp/minimal_rfuse3_test &
FS_PID=$!

# 等待启动
echo "等待文件系统启动..."
sleep 3

# 检查是否成功启动
if ! kill -0 $FS_PID 2>/dev/null; then
    echo "❌ 文件系统启动失败"
    exit 1
fi

echo "✅ 文件系统启动成功 (PID: $FS_PID)"

# 测试功能
echo "=== 测试文件系统功能 ==="

echo "测试 1: 列出目录内容"
if ls -la /tmp/minimal_rfuse3_test >/dev/null 2>&1; then
    echo "✅ 目录列表成功"
    ls -la /tmp/minimal_rfuse3_test
else
    echo "❌ 目录列表失败"
fi

echo ""
echo "测试 2: 读取文件内容"
if cat /tmp/minimal_rfuse3_test/hello.txt >/dev/null 2>&1; then
    echo "✅ 文件读取成功"
    echo "文件内容:"
    cat /tmp/minimal_rfuse3_test/hello.txt
else
    echo "❌ 文件读取失败"
fi

echo ""
echo "测试 3: 检查文件属性"
if stat /tmp/minimal_rfuse3_test/hello.txt >/dev/null 2>&1; then
    echo "✅ 文件属性获取成功"
    stat /tmp/minimal_rfuse3_test/hello.txt
else
    echo "❌ 文件属性获取失败"
fi

echo ""
echo "=== 交互测试 ==="
echo "文件系统正在运行，您可以手动测试："
echo "  - ls -la /tmp/minimal_rfuse3_test"
echo "  - cat /tmp/minimal_rfuse3_test/hello.txt"
echo "  - stat /tmp/minimal_rfuse3_test/hello.txt"
echo ""
echo "按 Enter 键停止文件系统..."

# 等待用户输入
read -r

# 清理
echo "停止文件系统..."
kill $FS_PID 2>/dev/null || true
sleep 1

echo "卸载文件系统..."
fusermount -u /tmp/minimal_rfuse3_test 2>/dev/null || true

echo "清理挂载点..."
rmdir /tmp/minimal_rfuse3_test 2>/dev/null || true

echo "✅ 测试完成！"
