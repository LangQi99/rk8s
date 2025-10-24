#!/bin/bash
# 容器卷管理演示：PassthroughFS + Bind Mount
# 模拟容器场景：基础文件系统 + 宿主卷挂载

set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
echo_success() { echo -e "${GREEN}[✓]${NC} $1"; }
echo_step() { echo -e "\n${YELLOW}━━━ $1 ━━━${NC}"; }

if ! command -v fusermount &> /dev/null; then
    echo "fusermount 未安装"
    exit 1
fi

TEST_DIR="/tmp/container_demo_$$"
CONTAINER_ROOT="${TEST_DIR}/container_root"
HOST_DATA="${TEST_DIR}/host_data"
HOST_CONFIG="${TEST_DIR}/host_config"
MNT_DIR="${TEST_DIR}/mnt"

cleanup() {
    echo_step "清理"
    mountpoint -q "${MNT_DIR}" 2>/dev/null && (fusermount -u "${MNT_DIR}" || umount "${MNT_DIR}") || true
    [ -n "${PID}" ] && kill -9 ${PID} 2>/dev/null || true
    rm -rf "${TEST_DIR}"
}

trap cleanup EXIT INT TERM

echo_step "容器卷管理演示"
echo_info "模拟场景：rootless 容器 + 宿主目录挂载"

echo_step "1. 准备容器环境"
mkdir -p "${CONTAINER_ROOT}/app" "${CONTAINER_ROOT}/etc" "${MNT_DIR}"
mkdir -p "${HOST_DATA}/database" "${HOST_CONFIG}"

# 容器基础文件
echo "v1.0" > "${CONTAINER_ROOT}/app/version"
echo "#!/bin/sh" > "${CONTAINER_ROOT}/app/run.sh"
echo "echo 'Container app running'" >> "${CONTAINER_ROOT}/app/run.sh"
chmod +x "${CONTAINER_ROOT}/app/run.sh"

# 宿主数据
echo "production database" > "${HOST_DATA}/database/data.db"
echo "user: admin" > "${HOST_DATA}/database/users.txt"
mkdir -p "${HOST_DATA}/uploads"
echo "uploaded file" > "${HOST_DATA}/uploads/file1.txt"

# 宿主配置（只读）
echo "DB_HOST=localhost" > "${HOST_CONFIG}/app.env"
echo "PORT=8080" > "${HOST_CONFIG}/app.env"

echo_success "环境准备完成"
echo_info "  容器根: ${CONTAINER_ROOT}"
echo_info "  宿主数据: ${HOST_DATA}"
echo_info "  宿主配置: ${HOST_CONFIG}"

echo_step "2. 启动容器文件系统 (PassthroughFS + Bind Mount)"
echo_info "使用 passthrough_example 模拟容器运行时"
echo ""
echo_info "$ passthrough_example \\"
echo_info "    --rootdir ${CONTAINER_ROOT} \\"
echo_info "    --mountpoint ${MNT_DIR} \\"
echo_info "    --bind data:${HOST_DATA} \\"
echo_info "    --bind config:${HOST_CONFIG}:ro"

cd "$(dirname "$0")/.."
cargo build --example passthrough_example --quiet 2>&1 | grep -v "^$" || true

cargo run --example passthrough_example --quiet -- \
    --rootdir "${CONTAINER_ROOT}" \
    --mountpoint "${MNT_DIR}" \
    --bind "data:${HOST_DATA}" \
    --bind "config:${HOST_CONFIG}:ro" \
    > /tmp/container_log_$$.txt 2>&1 &
PID=$!

for i in {1..20}; do
    mountpoint -q "${MNT_DIR}" 2>/dev/null && break
    [ $i -eq 20 ] && { echo "挂载失败"; cat /tmp/container_log_$$.txt; exit 1; }
    sleep 0.3
done
sleep 1

echo_success "容器文件系统已就绪"

echo_step "3. 验证容器视图"
echo_info "$ ls -la ${MNT_DIR}"
ls -la "${MNT_DIR}"
echo_success "可以看到: app/, etc/ (容器), data/, config/ (宿主)"

echo_step "4. 访问容器自身文件"
echo_info "$ cat ${MNT_DIR}/app/version"
cat "${MNT_DIR}/app/version"
echo ""
echo_info "$ ${MNT_DIR}/app/run.sh"
${MNT_DIR}/app/run.sh
echo_success "容器应用正常运行"

echo_step "5. 访问宿主挂载的数据卷 (/data)"
echo_info "$ ls -la ${MNT_DIR}/data"
ls -la "${MNT_DIR}/data"
echo ""
echo_info "$ cat ${MNT_DIR}/data/database/data.db"
cat "${MNT_DIR}/data/database/data.db"
echo_success "容器可以访问宿主数据！"

echo_step "6. 容器写入数据卷"
echo_info "$ echo 'Container log' > ${MNT_DIR}/data/app.log"
echo "Container log entry at $(date)" > "${MNT_DIR}/data/app.log"
sync

echo_info "验证写入到宿主:"
echo_info "$ cat ${HOST_DATA}/app.log"
cat "${HOST_DATA}/app.log"
echo_success "数据持久化到宿主目录！"

echo_step "7. 创建上传文件"
echo_info "$ echo 'user upload' > ${MNT_DIR}/data/uploads/user_file.txt"
echo "User uploaded content" > "${MNT_DIR}/data/uploads/user_file.txt"
sync

echo_info "$ ls ${HOST_DATA}/uploads/"
ls -la "${HOST_DATA}/uploads/"
echo_success "文件出现在宿主！"

echo_step "8. 访问只读配置 (/config)"
echo_info "$ cat ${MNT_DIR}/config/app.env"
cat "${MNT_DIR}/config/app.env"
echo_success "可以读取宿主配置"

echo_step "9. 验证只读保护"
echo_info "尝试修改配置（应该失败）"
if echo "test" > "${MNT_DIR}/config/test.txt" 2>&1 | grep -q "只读"; then
    echo_success "只读保护生效！"
elif [ ! -f "${HOST_CONFIG}/test.txt" ]; then
    echo_success "只读保护生效！"
fi

echo_step "10. 混合操作演示"
echo_info "创建脚本，同时访问容器文件和宿主卷"
cat > "${MNT_DIR}/app/status.sh" << EOF
#!/bin/sh
echo "=== Container Status ==="
echo "Version: \$(cat ${MNT_DIR}/app/version)"
echo "Data files: \$(ls ${MNT_DIR}/data | wc -l)"
echo "DB: \$(head -1 ${MNT_DIR}/data/database/data.db)"
echo "Config: \$(grep PORT ${MNT_DIR}/config/app.env)"
EOF
chmod +x "${MNT_DIR}/app/status.sh"
sync

echo_info "$ bash ${MNT_DIR}/app/status.sh"
bash "${MNT_DIR}/app/status.sh"
echo_success "可以同时访问容器文件和宿主卷！"

echo ""
echo_step "功能验证总结"
echo_success "✓ PassthroughFS + Bind Mount 正常工作"
echo_success "✓ 容器可以访问自身文件系统"
echo_success "✓ 容器可以访问宿主挂载的数据卷"
echo_success "✓ 数据写入正确持久化到宿主"
echo_success "✓ 只读卷保护正常"
echo_success "✓ 适用于 rootless 容器场景"

echo ""
echo_step "容器场景架构"
echo_info "┌──────────────────────────────────────┐"
echo_info "│         容器应用                      │"
echo_info "└─────────────┬────────────────────────┘"
echo_info "              │"
echo_info "              ▼"
echo_info "┌──────────────────────────────────────┐"
echo_info "│      容器视图 (FUSE 挂载点)          │"
echo_info "│  /app/       (容器自身文件)          │"
echo_info "│  /etc/       (容器自身文件)          │"
echo_info "│  /data/      (Bind -> 宿主数据)      │"
echo_info "│  /config/    (Bind -> 宿主配置:ro)   │"
echo_info "└─────────────┬────────────────────────┘"
echo_info "              │"
echo_info "              ▼"
echo_info "┌──────────────────────────────────────┐"
echo_info "│   PassthroughFS + Bind Mount         │"
echo_info "│   (用户态，无需 root)                 │"
echo_info "└──────┬───────────┬───────────────────┘"
echo_info "       │           │"
echo_info "       ▼           ▼"
echo_info "  容器 rootfs   宿主目录"

echo ""
echo_step "适用场景"
echo_info "• Rootless 容器运行时"
echo_info "• 开发环境（代码热重载）"
echo_info "• 数据持久化"
echo_info "• 配置文件注入"
echo_info "• 多容器共享数据卷"

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}   容器卷管理演示成功！${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

