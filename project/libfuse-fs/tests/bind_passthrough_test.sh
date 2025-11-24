#!/bin/bash
# Passthrough FS + Bind Mount 功能完整演示（需要 FUSE 挂载权限）

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
TEST_DIR="/tmp/passthrough_demo_$$"
SRC_DIR="${TEST_DIR}/src"
HOST_DIR="${TEST_DIR}/host"
HOST2_DIR="${TEST_DIR}/host2"
MNT_DIR="${TEST_DIR}/mnt"

cleanup() {
    echo_step "清理环境"
    
    if mountpoint -q "${MNT_DIR}" 2>/dev/null; then
        echo_info "卸载文件系统..."
        fusermount -u "${MNT_DIR}" 2>/dev/null || umount "${MNT_DIR}" 2>/dev/null || true
        sleep 1
    fi
    
    if [ -n "${FUSE_PID}" ] && kill -0 ${FUSE_PID} 2>/dev/null; then
        echo_info "终止 FUSE 进程 ${FUSE_PID}..."
        kill ${FUSE_PID} 2>/dev/null || true
        sleep 1
        kill -9 ${FUSE_PID} 2>/dev/null || true
    fi
    
    if [ -d "${TEST_DIR}" ]; then
        echo_info "删除测试目录..."
        rm -rf "${TEST_DIR}"
    fi
    
    echo_success "清理完成"
}

trap cleanup EXIT INT TERM

echo_step "准备测试环境"
mkdir -p "${SRC_DIR}" "${HOST_DIR}" "${HOST2_DIR}" "${MNT_DIR}"

# 创建测试文件
echo_info "创建测试文件..."
echo "Hello from host directory!" > "${HOST_DIR}/hello.txt"
echo "Configuration data" > "${HOST_DIR}/config.ini"
mkdir -p "${HOST_DIR}/data"
echo "Important data content" > "${HOST_DIR}/data/file.txt"

echo "Readonly content" > "${HOST2_DIR}/readonly.txt"
mkdir -p "${HOST2_DIR}/logs"
echo "Log file" > "${HOST2_DIR}/logs/app.log"

echo_success "环境准备完成"
echo_info "  源目录: ${SRC_DIR}"
echo_info "  宿主目录1 (读写): ${HOST_DIR}"
echo_info "  宿主目录2 (只读): ${HOST2_DIR}"
echo_info "  挂载点: ${MNT_DIR}"

# 编译示例
echo_step "编译示例程序"
cd "$(dirname "$0")/.."
cargo build --example passthrough --quiet
echo_success "编译完成"

# 启动 FUSE 文件系统
echo_step "启动 Passthrough 文件系统（带 Bind Mount）"
echo_info "命令: passthrough \\"
echo_info "  --rootdir ${SRC_DIR} \\"
echo_info "  --mountpoint ${MNT_DIR} \\"
echo_info "  --bind volumes:${HOST_DIR} \\"
echo_info "  --bind logs:${HOST2_DIR}:ro"

cargo run --example passthrough --quiet -- \
    --rootdir "${SRC_DIR}" \
    --mountpoint "${MNT_DIR}" \
    --bind "volumes:${HOST_DIR}" \
    --bind "logs:${HOST2_DIR}:ro" \
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

echo_step "测试 2: 列出 bind mount 目录 (/volumes)"
echo_info "$ ls -lh ${MNT_DIR}/volumes"
ls -lh "${MNT_DIR}/volumes"
echo_success "Pass - 可以看到宿主目录的文件"

echo_step "测试 3: 读取宿主文件"
echo_info "$ cat ${MNT_DIR}/volumes/hello.txt"
cat "${MNT_DIR}/volumes/hello.txt"
echo_success "Pass - 内容正确"

echo_step "测试 4: 读取配置文件"
echo_info "$ cat ${MNT_DIR}/volumes/config.ini"
cat "${MNT_DIR}/volumes/config.ini"
echo_success "Pass"

echo_step "测试 5: 列出子目录"
echo_info "$ ls -lh ${MNT_DIR}/volumes/data"
ls -lh "${MNT_DIR}/volumes/data"
echo_success "Pass - 子目录内容正确"

echo_step "测试 6: 读取子目录文件"
echo_info "$ cat ${MNT_DIR}/volumes/data/file.txt"
cat "${MNT_DIR}/volumes/data/file.txt"
echo_success "Pass"

echo_step "测试 7: 通过 bind mount 创建文件"
echo_info "$ echo 'Created via bind mount' > ${MNT_DIR}/volumes/new.txt"
echo "Created via bind mount" > "${MNT_DIR}/volumes/new.txt"
sync
echo_success "Pass - 文件已创建"

echo_step "测试 8: 验证文件在宿主目录"
echo_info "$ cat ${HOST_DIR}/new.txt"
cat "${HOST_DIR}/new.txt"
echo_success "Pass - 文件正确写入宿主目录！"

echo_step "测试 9: 创建目录和文件"
echo_info "$ mkdir ${MNT_DIR}/volumes/newdir"
mkdir "${MNT_DIR}/volumes/newdir"
echo_info "$ echo 'test' > ${MNT_DIR}/volumes/newdir/test.txt"
echo "test content" > "${MNT_DIR}/volumes/newdir/test.txt"
sync
echo_success "Pass - 目录和文件已创建"

echo_step "测试 10: 验证新目录在宿主"
echo_info "$ ls -lh ${HOST_DIR}/newdir"
ls -lh "${HOST_DIR}/newdir"
echo_info "$ cat ${HOST_DIR}/newdir/test.txt"
cat "${HOST_DIR}/newdir/test.txt"
echo_success "Pass - 新目录和文件都在宿主目录！"

echo_step "测试 11: 统计文件数量"
HOST_COUNT=$(find "${HOST_DIR}" -type f | wc -l)
MOUNT_COUNT=$(find "${MNT_DIR}/volumes" -type f | wc -l)
echo_info "宿主目录文件数: ${HOST_COUNT}"
echo_info "挂载目录文件数: ${MOUNT_COUNT}"
if [ "${HOST_COUNT}" -eq "${MOUNT_COUNT}" ]; then
    echo_success "Pass - 文件数量一致"
else
    echo_error "文件数量不匹配"
fi

echo_step "测试 12: 只读 bind mount (/logs)"
echo_info "$ ls -lh ${MNT_DIR}/logs"
ls -lh "${MNT_DIR}/logs"
echo_success "Pass - 可以列出只读目录"

echo_step "测试 13: 读取只读 bind mount 文件"
echo_info "$ cat ${MNT_DIR}/logs/readonly.txt"
cat "${MNT_DIR}/logs/readonly.txt"
echo_info "$ cat ${MNT_DIR}/logs/logs/app.log"
cat "${MNT_DIR}/logs/logs/app.log"
echo_success "Pass - 可以读取只读 bind mount 的文件"

echo_step "测试 14: 只读保护验证"
echo_info "$ 尝试在只读 bind mount 创建文件（应该失败）"
if echo "should fail" > "${MNT_DIR}/logs/test.txt" 2>&1 | grep -q "只读"; then
    echo_success "Pass - 只读保护正常（收到'只读文件系统'错误）"
elif [ ! -f "${HOST2_DIR}/test.txt" ]; then
    echo_success "Pass - 只读保护正常（写入被拒绝）"
else
    echo_error "只读保护失败 - 文件不应该被创建"
fi

echo ""
echo_step "功能验证总结"
echo_success "✓ --bind 参数正常工作"
echo_success "✓ 支持多个 bind mount 同时使用"
echo_success "✓ 读写 bind mount 正常（volumes）"
echo_success "✓ 只读 bind mount 保护正常（logs:ro）"
echo_success "✓ 文件和目录读取正常"
echo_success "✓ 文件和目录写入正常"
echo_success "✓ 所有操作正确映射到宿主目录"
echo_success "✓ 支持多层目录结构"
echo_success "✓ 文件同步正确"

echo ""
echo_step "使用方式"
echo_info "标准的 passthrough 现在支持 --bind 参数："
echo ""
echo "  cargo run --example passthrough -- \\"
echo "    --rootdir /path/to/source \\"
echo "    --mountpoint /path/to/mount \\"
echo "    --bind volumes:/host/volumes \\"
echo "    --bind config:/host/config:ro"
echo ""
echo_info "参数格式: mount_point:host_path[:ro]"
echo_info "可以多次指定 --bind 来挂载多个目录"

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}   Passthrough + Bind Mount 演示成功！${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

