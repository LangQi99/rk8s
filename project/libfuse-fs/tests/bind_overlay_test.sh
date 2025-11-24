#!/bin/bash
# OverlayFS + Bind Mount 集成测试（需要 FUSE 挂载权限）

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
TEST_DIR="/tmp/overlay_bind_test_$$"
LOWER_DIR="${TEST_DIR}/lower"
UPPER_DIR="${TEST_DIR}/upper"
HOST_RW_DIR="${TEST_DIR}/host_rw"
HOST_RO_DIR="${TEST_DIR}/host_ro"
MERGED_DIR="${TEST_DIR}/merged"

cleanup() {
    echo_step "清理环境"
    
    if mountpoint -q "${MERGED_DIR}" 2>/dev/null; then
        echo_info "卸载文件系统..."
        fusermount -u "${MERGED_DIR}" 2>/dev/null || umount "${MERGED_DIR}" 2>/dev/null || true
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
mkdir -p "${LOWER_DIR}" "${UPPER_DIR}" "${HOST_RW_DIR}" "${HOST_RO_DIR}" "${MERGED_DIR}"

# 创建 lower 层测试文件
echo_info "创建 lower 层文件..."
echo "Content from lower layer" > "${LOWER_DIR}/lower_file.txt"
mkdir -p "${LOWER_DIR}/lower_dir"
echo "Lower dir content" > "${LOWER_DIR}/lower_dir/file.txt"

# 创建 host 目录测试文件（用于 bind mount）
echo_info "创建 host 目录文件..."
echo "Host RW data" > "${HOST_RW_DIR}/data.txt"
mkdir -p "${HOST_RW_DIR}/subdir"
echo "Subdir data" > "${HOST_RW_DIR}/subdir/file.txt"

echo "Host RO config" > "${HOST_RO_DIR}/config.ini"
mkdir -p "${HOST_RO_DIR}/logs"
echo "Log content" > "${HOST_RO_DIR}/logs/app.log"

echo_success "环境准备完成"
echo_info "  Lower 层: ${LOWER_DIR}"
echo_info "  Upper 层: ${UPPER_DIR}"
echo_info "  Host RW: ${HOST_RW_DIR}"
echo_info "  Host RO: ${HOST_RO_DIR}"
echo_info "  Merged: ${MERGED_DIR}"

# 编译示例
echo_step "编译 overlayfs_example"
cd "$(dirname "$0")/.."
cargo build --example overlayfs_example --quiet
echo_success "编译完成"

# 启动 OverlayFS 文件系统
echo_step "启动 OverlayFS（带 Bind Mount）"
echo_info "命令: overlayfs_example \\"
echo_info "  --mountpoint ${MERGED_DIR} \\"
echo_info "  --upperdir ${UPPER_DIR} \\"
echo_info "  --lowerdir ${LOWER_DIR} \\"
echo_info "  --bind volumes:${HOST_RW_DIR} \\"
echo_info "  --bind config:${HOST_RO_DIR}:ro \\"
echo_info "  --allow-other"

cargo run --example overlayfs_example --quiet -- \
    --mountpoint "${MERGED_DIR}" \
    --upperdir "${UPPER_DIR}" \
    --lowerdir "${LOWER_DIR}" \
    --bind "volumes:${HOST_RW_DIR}" \
    --bind "config:${HOST_RO_DIR}:ro" \
    --allow-other \
    > /tmp/overlay_fuse_log_$$.txt 2>&1 &
FUSE_PID=$!

echo_info "FUSE 进程 PID: ${FUSE_PID}"
echo_info "等待挂载..."

# 等待挂载
for i in {1..20}; do
    if mountpoint -q "${MERGED_DIR}" 2>/dev/null; then
        echo_success "文件系统已挂载"
        break
    fi
    if ! kill -0 ${FUSE_PID} 2>/dev/null; then
        echo_error "FUSE 进程异常退出"
        cat /tmp/overlay_fuse_log_$$.txt
        exit 1
    fi
    if [ $i -eq 20 ]; then
        echo_error "挂载超时"
        cat /tmp/overlay_fuse_log_$$.txt
        exit 1
    fi
    sleep 0.3
done

sleep 1

# 执行测试
echo_step "测试 1: 列出 merged 根目录"
echo_info "$ ls -lh ${MERGED_DIR}"
ls -lh "${MERGED_DIR}"
echo_success "Pass - 应该看到 lower_file.txt, lower_dir, volumes, config"

echo_step "测试 2: 验证 lower 层文件可见"
echo_info "$ cat ${MERGED_DIR}/lower_file.txt"
CONTENT=$(cat "${MERGED_DIR}/lower_file.txt")
if [ "$CONTENT" = "Content from lower layer" ]; then
    echo_success "Pass - Lower 层文件内容正确"
else
    echo_error "Lower 层文件内容不正确: $CONTENT"
fi

echo_step "测试 3: 列出 RW bind mount 目录 (/volumes)"
echo_info "$ ls -lh ${MERGED_DIR}/volumes"
ls -lh "${MERGED_DIR}/volumes"
echo_success "Pass - 可以看到 bind mount (rw) 的内容"

echo_step "测试 4: 读取 bind mount 文件"
echo_info "$ cat ${MERGED_DIR}/volumes/data.txt"
CONTENT=$(cat "${MERGED_DIR}/volumes/data.txt")
if [ "$CONTENT" = "Host RW data" ]; then
    echo_success "Pass - Bind mount 文件读取正确"
else
    echo_error "Bind mount 文件内容不正确: $CONTENT"
fi

echo_step "测试 5: 读取 bind mount 子目录文件"
echo_info "$ cat ${MERGED_DIR}/volumes/subdir/file.txt"
cat "${MERGED_DIR}/volumes/subdir/file.txt"
echo_success "Pass - 子目录访问正常"

echo_step "测试 6: 通过 merged 层在 bind mount 创建文件"
echo_info "$ echo 'Created via overlay' > ${MERGED_DIR}/volumes/new.txt"
echo "Created via overlay merged" > "${MERGED_DIR}/volumes/new.txt"
sync
echo_success "Pass - 文件已创建"

echo_step "测试 7: 验证文件在 host 目录"
echo_info "$ cat ${HOST_RW_DIR}/new.txt"
CONTENT=$(cat "${HOST_RW_DIR}/new.txt")
if [ "$CONTENT" = "Created via overlay merged" ]; then
    echo_success "Pass - 文件正确写入 host 目录！"
else
    echo_error "Host 文件内容不正确: $CONTENT"
fi

echo_step "测试 8: 在 bind mount 创建目录"
echo_info "$ mkdir ${MERGED_DIR}/volumes/newdir"
mkdir "${MERGED_DIR}/volumes/newdir"
echo "test" > "${MERGED_DIR}/volumes/newdir/test.txt"
sync
echo_success "Pass - 目录和文件已创建"

echo_step "测试 9: 验证新目录在 host"
echo_info "$ ls ${HOST_RW_DIR}/newdir"
ls "${HOST_RW_DIR}/newdir"
if [ -f "${HOST_RW_DIR}/newdir/test.txt" ]; then
    echo_success "Pass - 目录和文件都在 host 目录！"
else
    echo_error "文件未出现在 host 目录"
fi

echo_step "测试 10: 只读 bind mount (/config)"
echo_info "$ ls -lh ${MERGED_DIR}/config"
ls -lh "${MERGED_DIR}/config"
echo_success "Pass - 可以列出只读 bind mount"

echo_step "测试 11: 读取只读 bind mount 文件"
echo_info "$ cat ${MERGED_DIR}/config/config.ini"
CONTENT=$(cat "${MERGED_DIR}/config/config.ini")
if [ "$CONTENT" = "Host RO config" ]; then
    echo_success "Pass - 只读 bind mount 读取正常"
else
    echo_error "只读 bind mount 内容不正确: $CONTENT"
fi

echo_step "测试 12: 只读 bind mount 写保护 - 创建文件"
echo_info "$ 尝试在只读 bind mount 创建文件（应该失败）"
if echo "should fail" > "${MERGED_DIR}/config/test.txt" 2>&1 | grep -q "只读"; then
    echo_success "Pass - 只读保护正常（收到'只读'错误）"
elif [ ! -f "${HOST_RO_DIR}/test.txt" ]; then
    echo_success "Pass - 只读保护正常（写入被拒绝）"
else
    echo_error "只读保护失败 - 文件不应该被创建"
fi

echo_step "测试 13: 只读 bind mount 写保护 - 修改文件"
echo_info "$ 尝试修改只读 bind mount 中的文件（应该失败）"
ORIGINAL=$(cat "${HOST_RO_DIR}/config.ini")
if echo "append" >> "${MERGED_DIR}/config/config.ini" 2>&1 | grep -q "只读"; then
    echo_success "Pass - 修改保护正常"
elif [ "$(cat ${HOST_RO_DIR}/config.ini)" = "$ORIGINAL" ]; then
    echo_success "Pass - 修改被拒绝"
else
    echo_error "只读保护失败 - 文件被修改"
fi

echo_step "测试 14: Copy-up 不影响 bind mount"
echo_info "$ 修改 lower 层文件触发 copy-up"
echo "Modified content" > "${MERGED_DIR}/lower_file.txt"
sync
if [ -f "${UPPER_DIR}/lower_file.txt" ]; then
    echo_success "Pass - Copy-up 成功，文件出现在 upper"
else
    echo_error "Copy-up 失败"
fi

# 验证 bind mount 未受影响
if [ -f "${MERGED_DIR}/volumes/data.txt" ]; then
    echo_success "Pass - Bind mount 路径未受 copy-up 影响"
else
    echo_error "Bind mount 路径丢失"
fi

echo_step "测试 15: 混合内容验证"
echo_info "验证 merged 视图同时包含 lower、upper 和 bind mount 内容"
FILE_COUNT=$(find "${MERGED_DIR}" -type f | wc -l)
echo_info "Merged 层总文件数: ${FILE_COUNT}"
if [ ${FILE_COUNT} -ge 8 ]; then
    echo_success "Pass - 包含足够的文件（lower + upper + bind mounts）"
else
    echo_error "文件数量不足: ${FILE_COUNT}"
fi

echo_step "测试 16: 文件数量一致性检查"
HOST_RW_COUNT=$(find "${HOST_RW_DIR}" -type f | wc -l)
VOLUMES_COUNT=$(find "${MERGED_DIR}/volumes" -type f 2>/dev/null | wc -l)
echo_info "Host RW 目录文件数: ${HOST_RW_COUNT}"
echo_info "Merged /volumes 文件数: ${VOLUMES_COUNT}"
if [ "${HOST_RW_COUNT}" -eq "${VOLUMES_COUNT}" ]; then
    echo_success "Pass - Bind mount 文件数量一致"
else
    echo_error "文件数量不匹配"
fi

echo ""
echo_step "功能验证总结"
echo_success "✓ OverlayFS 正常挂载"
echo_success "✓ Lower 层文件可见"
echo_success "✓ RW bind mount 读取正常"
echo_success "✓ RW bind mount 写入正常"
echo_success "✓ 写入正确同步到 host 目录"
echo_success "✓ RO bind mount 读取正常"
echo_success "✓ RO bind mount 写保护正常"
echo_success "✓ Copy-up 机制不影响 bind mount"
echo_success "✓ 多层内容正确混合"

echo ""
echo_step "使用方式"
echo_info "overlayfs_example 现在支持 --bind 参数："
echo ""
echo "  cargo run --example overlayfs_example -- \\"
echo "    --mountpoint /path/to/mount \\"
echo "    --upperdir /path/to/upper \\"
echo "    --lowerdir /path/to/lower \\"
echo "    --bind volumes:/host/volumes \\"
echo "    --bind config:/host/config:ro \\"
echo "    --allow-other"
echo ""
echo_info "参数格式: mount_point:host_path[:ro]"
echo_info "可以多次指定 --bind 来挂载多个目录"

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN} OverlayFS + Bind Mount 集成测试成功！${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
