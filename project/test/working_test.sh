#!/bin/bash

# rfuse3 最小化示例 - 可复现的测试脚本

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 配置
MOUNT_POINT="/tmp/rr"
BINARY="./target/debug/minimal_rfuse3_example"

echo -e "${BLUE}=== rfuse3 最小化示例测试 ===${NC}"

# 函数：打印步骤
print_step() {
    echo -e "${YELLOW}[步骤] $1${NC}"
}

# 函数：打印成功
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

# 函数：打印错误
print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# 函数：清理
cleanup() {
    print_step "清理环境"
    
    # 停止文件系统进程
    if pgrep -f "minimal_rfuse3_example" > /dev/null; then
        print_step "停止文件系统进程"
        pkill -f "minimal_rfuse3_example" || true
        sleep 1
    fi
    
    # 卸载文件系统
    if [ -d "$MOUNT_POINT" ]; then
        print_step "卸载文件系统"
        umount "$MOUNT_POINT" 2>/dev/null || true
        sleep 1
    fi
    
    # 删除挂载点
    if [ -d "$MOUNT_POINT" ]; then
        print_step "删除挂载点"
        rmdir "$MOUNT_POINT" 2>/dev/null || true
    fi
}

# 函数：测试文件系统
test_filesystem() {
    local test_name="$1"
    local command="$2"
    
    echo -e "${YELLOW}测试: $test_name${NC}"
    echo "执行: $command"
    
    if eval "$command" 2>/dev/null; then
        print_success "$test_name"
        return 0
    else
        print_error "$test_name"
        return 1
    fi
}

# 设置退出时清理
trap cleanup EXIT

# 步骤 1: 清理环境
cleanup

# 步骤 2: 构建示例
print_step "构建 rfuse3 示例"
if cargo build --bin minimal_rfuse3_example; then
    print_success "构建成功"
else
    print_error "构建失败"
    exit 1
fi

# 步骤 3: 创建挂载点
print_step "创建挂载点: $MOUNT_POINT"
mkdir -p "$MOUNT_POINT"
print_success "挂载点创建成功"

# 步骤 4: 启动文件系统
print_step "启动文件系统"
echo "运行: $BINARY --mountpoint $MOUNT_POINT"
$BINARY --mountpoint "$MOUNT_POINT" &
FS_PID=$!

# 等待文件系统启动
print_step "等待文件系统启动"
sleep 3

# 检查文件系统是否成功启动
if ! kill -0 $FS_PID 2>/dev/null; then
    print_error "文件系统启动失败"
    exit 1
fi

print_success "文件系统启动成功 (PID: $FS_PID)"

# 步骤 5: 测试文件系统功能
echo -e "${BLUE}=== 开始测试文件系统功能 ===${NC}"

# 测试 1: 列出目录内容
echo -e "${YELLOW}测试: 列出目录内容${NC}"
echo "执行: ls -la $MOUNT_POINT"
if ls -la "$MOUNT_POINT" 2>&1; then
    print_success "列出目录内容"
    echo "目录内容:"
    ls -la "$MOUNT_POINT" 2>&1
else
    print_error "列出目录内容"
    echo "错误信息:"
    ls -la "$MOUNT_POINT" 2>&1 || echo "命令执行失败"
fi

# 测试 2: 读取文件
if test_filesystem "读取 hello.txt 文件" "cat $MOUNT_POINT/hello.txt"; then
    echo "文件内容:"
    cat "$MOUNT_POINT/hello.txt" 2>/dev/null || echo "无法读取文件内容"
fi

# 测试 3: 检查文件属性
echo -e "${YELLOW}测试: 检查文件属性${NC}"
echo "执行: stat $MOUNT_POINT/hello.txt"
if stat "$MOUNT_POINT/hello.txt" 2>&1; then
    print_success "检查文件属性"
    echo "文件属性:"
    stat "$MOUNT_POINT/hello.txt" 2>&1
else
    print_error "检查文件属性"
    echo "错误信息:"
    stat "$MOUNT_POINT/hello.txt" 2>&1 || echo "命令执行失败"
fi

# 测试 4: 尝试写入（应该失败，因为是只读文件系统）
echo -e "${YELLOW}测试: 尝试写入文件（应该失败）${NC}"
echo "执行: echo 'test' > $MOUNT_POINT/hello.txt"
if echo 'test' > "$MOUNT_POINT/hello.txt" 2>/dev/null; then
    print_error "意外成功（文件系统应该是只读的）"
else
    print_success "正确失败（只读文件系统）"
fi

# 测试 5: 尝试创建文件（应该失败）
echo -e "${YELLOW}测试: 尝试创建文件（应该失败）${NC}"
echo "执行: touch $MOUNT_POINT/newfile.txt"
if touch "$MOUNT_POINT/newfile.txt" 2>/dev/null; then
    print_error "意外成功（文件系统应该是只读的）"
else
    print_success "正确失败（只读文件系统）"
fi

# 步骤 6: 显示最终状态
echo -e "${BLUE}=== 最终状态 ===${NC}"
echo "挂载点: $MOUNT_POINT"
echo "文件系统进程 PID: $FS_PID"
echo "目录内容:"
ls -la "$MOUNT_POINT" 2>/dev/null || echo "无法列出目录内容"

# 步骤 7: 用户交互
echo -e "${BLUE}=== 交互测试 ===${NC}"
echo "文件系统正在运行，您可以手动测试："
echo "  - ls -la $MOUNT_POINT"
echo "  - cat $MOUNT_POINT/hello.txt"
echo "  - stat $MOUNT_POINT/hello.txt"
echo ""
echo "按 Enter 键停止文件系统并清理环境..."

# 等待用户输入
read -r

# 步骤 8: 清理
print_step "停止文件系统并清理环境"
cleanup

echo -e "${GREEN}=== 测试完成！===${NC}"
echo "rfuse3 最小化示例测试成功完成！"
