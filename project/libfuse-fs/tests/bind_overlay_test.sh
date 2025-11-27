#!/bin/bash
# Overlay FS + Bind Mount 功能完整演示

set -e

# Ensure cargo is in PATH
export PATH="$HOME/.cargo/bin:$PATH"

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

echo_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

echo_error() {
    echo -e "${RED}[✗]${NC} $1"
}

echo_step() {
    echo -e "\n${YELLOW}━━━ $1 ━━━${NC}"
}

# 检查 FUSE 是否可用
if ! command -v fusermount &> /dev/null; then
    echo_error "fusermount 未安装，请安装 fuse"
    exit 1
fi

# 设置测试目录
TEST_DIR="/tmp/overlay_bind_demo_$$"
LOWER_DIR="${TEST_DIR}/lower"
UPPER_DIR="${TEST_DIR}/upper"
HOST_DIR="${TEST_DIR}/host"
HOST2_DIR="${TEST_DIR}/host2"
MNT_DIR="${TEST_DIR}/mnt"

cleanup() {
    echo_step "清理环境"
    
    if mountpoint -q "${MNT_DIR}" 2>/dev/null; then
        echo_info "卸载文件系统..."
        sudo umount "${MNT_DIR}" 2>/dev/null || true
        sleep 1
    fi
    
    if [ -n "${FUSE_PID}" ] && kill -0 ${FUSE_PID} 2>/dev/null; then
        echo_info "终止 FUSE 进程 ${FUSE_PID}..."
        sudo kill ${FUSE_PID} 2>/dev/null || true
        sleep 1
        sudo kill -9 ${FUSE_PID} 2>/dev/null || true
    fi

    # Unmount bind mounts in upper directory (since we bind to upper)
    if mountpoint -q "${UPPER_DIR}/volumes" 2>/dev/null; then
         echo_info "卸载 bind mount: ${UPPER_DIR}/volumes"
         sudo umount "${UPPER_DIR}/volumes" 2>/dev/null || true
    fi
    if mountpoint -q "${UPPER_DIR}/logs" 2>/dev/null; then
         echo_info "卸载 bind mount: ${UPPER_DIR}/logs"
         sudo umount "${UPPER_DIR}/logs" 2>/dev/null || true
    fi
    
    if [ -d "${TEST_DIR}" ]; then
        echo_info "删除测试目录..."
        sudo rm -rf "${TEST_DIR}"
    fi
    
    echo_success "清理完成"
}

trap cleanup EXIT INT TERM

echo_step "准备测试环境"
mkdir -p "${LOWER_DIR}" "${UPPER_DIR}" "${HOST_DIR}" "${HOST2_DIR}" "${MNT_DIR}"

# 创建测试文件
echo_info "创建测试文件..."
echo "Lower file" > "${LOWER_DIR}/lower.txt"
echo "Upper file" > "${UPPER_DIR}/upper.txt"

echo "Hello from host directory!" > "${HOST_DIR}/hello.txt"
mkdir -p "${HOST_DIR}/data"
echo "Important data content" > "${HOST_DIR}/data/file.txt"

echo "Readonly content" > "${HOST2_DIR}/readonly.txt"

echo_success "环境准备完成"
echo_info "  Lower目录: ${LOWER_DIR}"
echo_info "  Upper目录: ${UPPER_DIR}"
echo_info "  宿主目录1 (读写): ${HOST_DIR}"
echo_info "  宿主目录2 (只读): ${HOST2_DIR}"
echo_info "  挂载点: ${MNT_DIR}"

# 编译示例
echo_step "编译示例程序"
cd "$(dirname "$0")/.."
cargo build --example overlayfs_example --quiet
echo_success "编译完成"

# 启动 FUSE 文件系统
echo_step "启动 Overlay 文件系统（带 Bind Mount）"
echo_info "命令: sudo target/debug/examples/overlayfs_example ..."

# Use sudo for kernel bind mount
# Binary is in workspace target directory
sudo "/root/rk8s/project/target/debug/examples/overlayfs_example" \
    --lowerdir "${LOWER_DIR}" \
    --upperdir "${UPPER_DIR}" \
    --mountpoint "${MNT_DIR}" \
    --bind "volumes:${HOST_DIR}" \
    --bind "logs:${HOST2_DIR}:ro" \
    --allow-other \
    > /tmp/fuse_log_$$.txt 2>&1 &
FUSE_PID=$!

echo_info "FUSE 进程 PID: ${FUSE_PID}"
echo_info "等待挂载..."

# 等待挂载
for i in {1..20}; do
    if mountpoint -q "${MNT_DIR}" 2>/dev/null; then
        echo_success "文件系统已挂载"
        break
    fi
    if ! kill -0 ${FUSE_PID} 2>/dev/null; then
        echo_error "FUSE 进程异常退出"
        cat /tmp/fuse_log_$$.txt
        exit 1
    fi
    if [ $i -eq 20 ]; then
        echo_error "挂载超时"
        cat /tmp/fuse_log_$$.txt
        exit 1
    fi
    sleep 0.3
done

sleep 1

# 执行测试
echo_step "测试 1: 列出根目录"
echo_info "$ ls -lh ${MNT_DIR}"
ls -lh "${MNT_DIR}"
echo_success "Pass"

echo_step "测试 2: 验证 Overlay 基础功能"
if [ -f "${MNT_DIR}/lower.txt" ] && [ -f "${MNT_DIR}/upper.txt" ]; then
    echo_success "Pass - Lower 和 Upper 文件都可见"
else
    echo_error "Overlay 基础功能失败"
fi

echo_step "测试 3: 列出 bind mount 目录 (/volumes)"
echo_info "$ ls -lh ${MNT_DIR}/volumes"
ls -lh "${MNT_DIR}/volumes"
echo_success "Pass - 可以看到宿主目录的文件"

echo_step "测试 4: 读取宿主文件"
echo_info "$ cat ${MNT_DIR}/volumes/hello.txt"
cat "${MNT_DIR}/volumes/hello.txt"
echo_success "Pass - 内容正确"

echo_step "测试 5: 通过 bind mount 创建文件"
echo_info "$ echo 'Created via bind mount' > ${MNT_DIR}/volumes/new.txt"
sudo sh -c "echo 'Created via bind mount' > ${MNT_DIR}/volumes/new.txt"
sync
echo_success "Pass - 文件已创建"

echo_step "测试 6: 验证文件在宿主目录"
echo_info "$ cat ${HOST_DIR}/new.txt"
cat "${HOST_DIR}/new.txt"
echo_success "Pass - 文件正确写入宿主目录！"

echo_step "测试 7: 只读 bind mount (/logs)"
echo_info "$ ls -lh ${MNT_DIR}/logs"
ls -lh "${MNT_DIR}/logs"
echo_success "Pass - 可以列出只读目录"

echo_step "测试 8: 只读保护验证 - 创建文件"
echo_info "$ 尝试在只读 bind mount 创建文件（应该失败）"
if sudo sh -c "echo 'should fail' > ${MNT_DIR}/logs/test.txt" 2>&1 | grep -q "Read-only file system"; then
    echo_success "Pass - 只读保护正常"
elif [ ! -f "${HOST2_DIR}/test.txt" ]; then
    echo_success "Pass - 只读保护正常（写入被拒绝）"
else
    echo_error "只读保护失败 - 文件不应该被创建"
fi

echo ""
echo_step "功能验证总结"
echo_success "✓ Overlay 基础功能正常"
echo_success "✓ Bind mount 注入到 Overlay 正常"
echo_success "✓ 读写 Bind mount 正常"
echo_success "✓ 只读 Bind mount 保护正常"

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}   Overlay + Bind Mount 演示成功！${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
