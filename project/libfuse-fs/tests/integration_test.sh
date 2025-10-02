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

info() { echo "[INFO] $*"; }
warn() { echo "[WARN] $*"; }
err()  { echo "[ERROR] $*" >&2; }

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
	run_cmd rm -rf "$WORK_DIR" "$LOG_DIR"
	run_cmd mkdir -p "$OVL_MNT" "$OVL_UP" "$OVL_L1" "$OVL_L2" "$PT_SRC" "$PT_MNT" "$LOG_DIR"
	run_cmd echo "l1-data" >"$OVL_L1/file_a"
	run_cmd echo "l2-data" >"$OVL_L2/file_b"
	run_cmd echo "hello" >"$PT_SRC/hello.txt"
}

build_examples() {
	info "Building examples"
	# 在sudo环境下设置必要的环境变量，使用原始用户的home目录
	local user_home=$(eval echo ~$SUDO_USER 2>/dev/null || echo "$HOME")
	local cargo_bin="$user_home/.cargo/bin/cargo"
	run_cmd bash -c "cd '$CRATE_DIR' && export RUSTUP_HOME='$user_home/.rustup' && export CARGO_HOME='$user_home/.cargo' && '$cargo_bin' build --examples --quiet"
}

start_overlay() {
	info "Starting overlay example"
	local run_log="$LOG_DIR/overlay.run.log"
	run_cmd "$REPO_ROOT/project/target/debug/examples/overlayfs_example" \
		--mountpoint "$OVL_MNT" --upperdir "$OVL_UP" --lowerdir "$OVL_L1" --lowerdir "$OVL_L2" \
		>"$run_log" 2>&1 & echo $! >"$WORK_DIR/overlay.pid"
	run_cmd sleep 2
	if mountpoint -q "$OVL_MNT"; then
		info "Overlay mounted"
	else
		warn "Overlay mount failed (see $run_log)"
		return 1
	fi
}

start_passthrough() {
	info "Starting passthrough example"
	local run_log="$LOG_DIR/passthrough.run.log"
	run_cmd "$REPO_ROOT/project/target/debug/examples/passthrough_example" \
		--mountpoint "$PT_MNT" --rootdir "$PT_SRC" \
		>"$run_log" 2>&1 & echo $! >"$WORK_DIR/passthrough.pid"
	run_cmd sleep 2
	if mountpoint -q "$PT_MNT"; then
		info "Passthrough mounted"
	else
		warn "Passthrough mount failed (see $run_log)"
		return 1
	fi
}

run_ior() {
	local target=$1 tag=$2
	if ! find_ior; then warn "ior not found; skip $tag"; return 0; fi
	info "IOR on $tag"
	run_cmd "$IOR_BIN" -a POSIX -b 2m -t 1m -s 1 -F -o "$target/ior_file" -w -r -k -Q 1 -g -G 1 -v \
		>>"$LOG_DIR/ior-$tag.log" 2>&1 || warn "IOR failed on $tag"
}

run_fio() {
	local target=$1 tag=$2
	if ! command -v fio >/dev/null 2>&1; then warn "fio not found; skip $tag"; return 0; fi
	info "fio on $tag"
	run_cmd fio --name=seq_write --directory="$target" --filename=fiotest.dat --size=8M --bs=1M --rw=write --ioengine=sync --numjobs=1 \
		>>"$LOG_DIR/fio-$tag.log" 2>&1 || true
	run_cmd fio --name=seq_read  --directory="$target" --filename=fiotest.dat --size=8M --bs=1M --rw=read  --ioengine=sync --numjobs=1 \
		>>"$LOG_DIR/fio-$tag.log" 2>&1 || true
	run_cmd fio --name=randrw    --directory="$target" --filename=fiotest-rand.dat --size=8M --bs=4k --rw=randrw --rwmixread=50 --ioengine=sync --runtime=5 --time_based=1 \
		>>"$LOG_DIR/fio-$tag.log" 2>&1 || true
}

kill_and_unmount() {
	local pidf=$1 mnt=$2
	if [[ -f $pidf ]]; then
		local pid=$(cat "$pidf" || true)
		run_cmd kill "$pid" 2>/dev/null || true
		run_cmd sleep 1
	fi
	if mountpoint -q "$mnt"; then
		run_cmd fusermount3 -u "$mnt" 2>/dev/null || run_cmd sudo env PATH="$PATH" fusermount3 -u "$mnt" 2>/dev/null || true
	fi
}

main() {
	info "Artifact root: $ARTIFACT_ROOT"
	info "Work dir: $WORK_DIR"
	prepare_dirs
	build_examples

	start_overlay || info "Skip overlay workloads"
	if mountpoint -q "$OVL_MNT"; then
		run_ior "$OVL_MNT" overlay
		run_fio "$OVL_MNT" overlay
	fi

	start_passthrough || info "Skip passthrough workloads"
	if mountpoint -q "$PT_MNT"; then
		run_ior "$PT_MNT" passthrough
		run_fio "$PT_MNT" passthrough
	fi

	kill_and_unmount "$WORK_DIR/overlay.pid" "$OVL_MNT"
	kill_and_unmount "$WORK_DIR/passthrough.pid" "$PT_MNT"
	info "Logs: $LOG_DIR"
	info "All artifacts collected under: $ARTIFACT_ROOT"
	run_cmd ls -1 "$LOG_DIR" || true
}

trap 'echo "[CLEANUP] finishing"' EXIT
main "$@"
