#!/usr/bin/env bash
# Simplified integration test script for libfuse-fs
# Tasks:
#  1. Build examples (overlayfs_example, passthrough_example)
#  2. Start each filesystem
#  3. Run small fio & ior workloads inside the mounted directories
#  4. Save logs (no JSON parsing, no auto dependency install/build)

set -euo pipefail

# 确保PATH包含cargo路径，特别是当使用sudo运行时
export PATH="$HOME/.cargo/bin:$PATH"
export RUSTUP_HOME="$HOME/.rustup"

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
CRATE_DIR=$(cd "$SCRIPT_DIR/.." && pwd)          # project/libfuse-fs
REPO_ROOT=$(cd "$CRATE_DIR/../.." && pwd)
ARTIFACT_ROOT=${ARTIFACT_ROOT:-"$SCRIPT_DIR/test_artifacts"}  # 所有产物统一放这里
WORK_DIR="$ARTIFACT_ROOT/work"                                # 运行期工作目录
LOG_DIR="$ARTIFACT_ROOT/logs"
mkdir -p "$LOG_DIR" "$WORK_DIR"

OVERLAY_ROOT="$WORK_DIR/overlay"
PT_ROOT="$WORK_DIR/passthrough"
OVL_MNT="$OVERLAY_ROOT/mnt"; OVL_UP="$OVERLAY_ROOT/upper"; OVL_L1="$OVERLAY_ROOT/l1"; OVL_L2="$OVERLAY_ROOT/l2"
PT_SRC="$PT_ROOT/src"; PT_MNT="$PT_ROOT/mnt"

IOR_BIN=${IOR_BIN:-}

info() { echo "[INFO] $(date '+%H:%M:%S') $*"; }
warn() { echo "[WARN] $(date '+%H:%M:%S') $*"; }
err()  { echo "[ERROR] $(date '+%H:%M:%S') $*" >&2; }
step() { echo -e "\033[1;34m[STEP] $(date '+%H:%M:%S') $*\033[0m"; }
success() { echo -e "\033[1;32m[SUCCESS] $(date '+%H:%M:%S') $*\033[0m"; }

# 显示并执行命令
run_cmd() {
	# 使用彩色高亮显示命令（绿色）
	echo -e "\033[1;32m[CMD]\033[0m $*"
	"$@"
}

find_ior() {
	# 优先使用固定路径: project/libfuse-fs/tests/ior (必须是单个可执行文件)
	if [[ -n "$IOR_BIN" && -x "$IOR_BIN" ]]; then return 0; fi
	local fixed="$CRATE_DIR/tests/ior"
	if [[ -f "$fixed" ]]; then
		# 尝试赋予执行权限
		[[ -x "$fixed" ]] || run_cmd chmod +x "$fixed" 2>/dev/null || true
		if [[ -x "$fixed" ]]; then
			IOR_BIN="$fixed"; return 0
		fi
	fi
	# 退回到 PATH
	if command -v ior >/dev/null 2>&1; then IOR_BIN=$(command -v ior); return 0; fi
	return 1
}

prepare_dirs() {
	step "准备测试目录结构"
	info "清理旧的工作目录和日志目录..."
	echo -e "\033[1;36m[CLEAN CMD]\033[0m rm -rf '$WORK_DIR' '$LOG_DIR'"
	run_cmd rm -rf "$WORK_DIR" "$LOG_DIR"
	info "创建目录结构..."
	echo -e "\033[1;36m[MKDIR CMD]\033[0m mkdir -p '$OVL_MNT' '$OVL_UP' '$OVL_L1' '$OVL_L2' '$PT_SRC' '$PT_MNT' '$LOG_DIR'"
	run_cmd mkdir -p "$OVL_MNT" "$OVL_UP" "$OVL_L1" "$OVL_L2" "$PT_SRC" "$PT_MNT" "$LOG_DIR"
	info "创建测试文件..."
	echo -e "\033[1;36m[FILE CMD]\033[0m echo 'l1-data' > '$OVL_L1/file_a'"
	run_cmd echo "l1-data" >"$OVL_L1/file_a"
	echo -e "\033[1;36m[FILE CMD]\033[0m echo 'l2-data' > '$OVL_L2/file_b'"
	run_cmd echo "l2-data" >"$OVL_L2/file_b"
	echo -e "\033[1;36m[FILE CMD]\033[0m echo 'hello' > '$PT_SRC/hello.txt'"
	run_cmd echo "hello" >"$PT_SRC/hello.txt"
	success "目录结构准备完成"
}

build_examples() {
	step "构建示例程序"
	# 在sudo环境下设置必要的环境变量，使用原始用户的home目录
	local user_home=$(eval echo ~$SUDO_USER 2>/dev/null || echo "$HOME")
	local cargo_bin="$user_home/.cargo/bin/cargo"
	info "使用cargo构建overlayfs_example和passthrough_example..."
	info "工作目录: $CRATE_DIR"
	info "Cargo路径: $cargo_bin"
	info "执行构建命令:"
	echo -e "\033[1;36m[BUILD CMD]\033[0m cd '$CRATE_DIR' && export RUSTUP_HOME='$user_home/.rustup' && export CARGO_HOME='$user_home/.cargo' && '$cargo_bin' build --examples --quiet"
	run_cmd bash -c "cd '$CRATE_DIR' && export RUSTUP_HOME='$user_home/.rustup' && export CARGO_HOME='$user_home/.cargo' && '$cargo_bin' build --examples --quiet"
	success "示例程序构建完成"
	info "构建产物位置: $REPO_ROOT/project/target/debug/examples/"
}

start_overlay() {
	step "启动overlay文件系统"
	local run_log="$LOG_DIR/overlay.run.log"
	info "启动overlayfs_example，挂载点: $OVL_MNT"
	info "日志文件: $run_log"
	info "目录结构:"
	info "  - 挂载点: $OVL_MNT"
	info "  - 上层目录: $OVL_UP"
	info "  - 下层目录1: $OVL_L1"
	info "  - 下层目录2: $OVL_L2"
	info "执行挂载命令:"
	echo -e "\033[1;36m[MOUNT CMD]\033[0m $REPO_ROOT/project/target/debug/examples/overlayfs_example --mountpoint '$OVL_MNT' --upperdir '$OVL_UP' --lowerdir '$OVL_L1' --lowerdir '$OVL_L2'"
	run_cmd "$REPO_ROOT/project/target/debug/examples/overlayfs_example" \
		--mountpoint "$OVL_MNT" --upperdir "$OVL_UP" --lowerdir "$OVL_L1" --lowerdir "$OVL_L2" \
		>"$run_log" 2>&1 & echo $! >"$WORK_DIR/overlay.pid"
	info "等待文件系统启动..."
	run_cmd sleep 2
	if mountpoint -q "$OVL_MNT"; then
		success "Overlay文件系统挂载成功"
		info "显示overlay启动日志:"
		run_cmd head -10 "$run_log" || true
	else
		err "Overlay文件系统挂载失败"
		info "查看错误日志:"
		run_cmd cat "$run_log" || true
		return 1
	fi
}

start_passthrough() {
	step "启动passthrough文件系统"
	local run_log="$LOG_DIR/passthrough.run.log"
	info "启动passthrough_example，挂载点: $PT_MNT"
	info "日志文件: $run_log"
	info "目录结构:"
	info "  - 挂载点: $PT_MNT"
	info "  - 源目录: $PT_SRC"
	info "执行挂载命令:"
	echo -e "\033[1;36m[MOUNT CMD]\033[0m $REPO_ROOT/project/target/debug/examples/passthrough_example --mountpoint '$PT_MNT' --rootdir '$PT_SRC'"
	run_cmd "$REPO_ROOT/project/target/debug/examples/passthrough_example" \
		--mountpoint "$PT_MNT" --rootdir "$PT_SRC" \
		>"$run_log" 2>&1 & echo $! >"$WORK_DIR/passthrough.pid"
	info "等待文件系统启动..."
	run_cmd sleep 2
	if mountpoint -q "$PT_MNT"; then
		success "Passthrough文件系统挂载成功"
		info "显示passthrough启动日志:"
		run_cmd head -10 "$run_log" || true
	else
		err "Passthrough文件系统挂载失败"
		info "查看错误日志:"
		run_cmd cat "$run_log" || true
		return 1
	fi
}

run_ior() {
	local target=$1 tag=$2
	if ! find_ior; then warn "ior not found; skip $tag"; return 0; fi
	step "运行IOR性能测试 - $tag"
	info "目标目录: $target"
	local log_file="$LOG_DIR/ior-$tag.log"
	info "IOR日志文件: $log_file"
	info "IOR可执行文件: $IOR_BIN"
	info "运行IOR测试 (写入->读取->验证)..."
	info "执行IOR命令:"
	echo -e "\033[1;36m[IOR CMD]\033[0m $IOR_BIN -a POSIX -b 2m -t 1m -s 1 -F -o '$target/ior_file' -w -r -k -Q 1 -g -G 1 -v"
	run_cmd "$IOR_BIN" -a POSIX -b 2m -t 1m -s 1 -F -o "$target/ior_file" -w -r -k -Q 1 -g -G 1 -v \
		>>"$log_file" 2>&1 || warn "IOR failed on $tag"
	if [[ -f "$log_file" ]]; then
		info "IOR测试完成，显示关键结果:"
		run_cmd grep -E "(write|read|bandwidth|IOPS)" "$log_file" | tail -10 || true
	fi
}

run_fio() {
	local target=$1 tag=$2
	if ! command -v fio >/dev/null 2>&1; then warn "fio not found; skip $tag"; return 0; fi
	step "运行FIO性能测试 - $tag"
	info "目标目录: $target"
	local log_file="$LOG_DIR/fio-$tag.log"
	info "FIO日志文件: $log_file"
	
	info "运行顺序写入测试..."
	echo -e "\033[1;36m[FIO CMD]\033[0m fio --name=seq_write --directory='$target' --filename=fiotest.dat --size=8M --bs=1M --rw=write --ioengine=sync --numjobs=1"
	run_cmd fio --name=seq_write --directory="$target" --filename=fiotest.dat --size=8M --bs=1M --rw=write --ioengine=sync --numjobs=1 \
		>>"$log_file" 2>&1 || true
	info "运行顺序读取测试..."
	echo -e "\033[1;36m[FIO CMD]\033[0m fio --name=seq_read --directory='$target' --filename=fiotest.dat --size=8M --bs=1M --rw=read --ioengine=sync --numjobs=1"
	run_cmd fio --name=seq_read  --directory="$target" --filename=fiotest.dat --size=8M --bs=1M --rw=read  --ioengine=sync --numjobs=1 \
		>>"$log_file" 2>&1 || true
	info "运行随机读写测试..."
	echo -e "\033[1;36m[FIO CMD]\033[0m fio --name=randrw --directory='$target' --filename=fiotest-rand.dat --size=8M --bs=4k --rw=randrw --rwmixread=50 --ioengine=sync --runtime=5 --time_based=1"
	run_cmd fio --name=randrw    --directory="$target" --filename=fiotest-rand.dat --size=8M --bs=4k --rw=randrw --rwmixread=50 --ioengine=sync --runtime=5 --time_based=1 \
		>>"$log_file" 2>&1 || true
	
	if [[ -f "$log_file" ]]; then
		info "FIO测试完成，显示关键结果:"
		run_cmd grep -E "(write|read|bw=|iops=)" "$log_file" | tail -10 || true
	fi
}

kill_and_unmount() {
	local pidf=$1 mnt=$2
	local name=$(basename "$mnt")
	step "清理$name文件系统"
	if [[ -f $pidf ]]; then
		local pid=$(cat "$pidf" || true)
		info "终止进程 PID: $pid"
		echo -e "\033[1;36m[KILL CMD]\033[0m kill $pid"
		run_cmd kill "$pid" 2>/dev/null || true
		run_cmd sleep 1
	fi
	if mountpoint -q "$mnt"; then
		info "卸载挂载点: $mnt"
		echo -e "\033[1;36m[UNMOUNT CMD]\033[0m fusermount3 -u '$mnt'"
		run_cmd fusermount3 -u "$mnt" 2>/dev/null || run_cmd sudo env PATH="$PATH" fusermount3 -u "$mnt" 2>/dev/null || true
	fi
	success "$name文件系统清理完成"
}

main() {
	step "开始libfuse-fs集成测试"
	info "测试产物目录: $ARTIFACT_ROOT"
	info "工作目录: $WORK_DIR"
	info "日志目录: $LOG_DIR"
	
	prepare_dirs
	build_examples

	start_overlay || info "跳过overlay测试"
	if mountpoint -q "$OVL_MNT"; then
		run_ior "$OVL_MNT" overlay
		run_fio "$OVL_MNT" overlay
	else
		warn "Overlay未挂载，跳过性能测试"
	fi

	start_passthrough || info "跳过passthrough测试"
	if mountpoint -q "$PT_MNT"; then
		run_ior "$PT_MNT" passthrough
		run_fio "$PT_MNT" passthrough
	else
		warn "Passthrough未挂载，跳过性能测试"
	fi

	kill_and_unmount "$WORK_DIR/overlay.pid" "$OVL_MNT"
	kill_and_unmount "$WORK_DIR/passthrough.pid" "$PT_MNT"
	
	step "测试完成，汇总结果"
	info "所有日志文件位于: $LOG_DIR"
	info "所有测试产物位于: $ARTIFACT_ROOT"
	
	if [[ -d "$LOG_DIR" ]]; then
		info "生成的日志文件:"
		run_cmd ls -la "$LOG_DIR" || true
		
		info "显示关键测试结果摘要:"
		for log_file in "$LOG_DIR"/*.log; do
			if [[ -f "$log_file" ]]; then
				local name=$(basename "$log_file")
				echo -e "\033[1;33m=== $name ===\033[0m"
				run_cmd head -5 "$log_file" || true
				echo ""
			fi
		done
	fi
	
	success "集成测试全部完成"
}

trap 'echo "[CLEANUP] $(date "+%H:%M:%S") 清理完成，退出测试"' EXIT
main "$@"
