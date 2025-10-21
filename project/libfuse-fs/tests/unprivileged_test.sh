#!/usr/bin/env bash
# 非特权模式运行 integration_test
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# 设置非特权模式
export USE_PRIVILEGED_MOUNT=false

# 运行 integration_test.sh
bash "$SCRIPT_DIR/integration_test.sh"

# 清理挂载点（如果需要）
WORK_DIR="$SCRIPT_DIR/test_artifacts/work"
fusermount3 -u "$WORK_DIR/passthrough/mnt" 2>/dev/null || true
fusermount3 -u "$WORK_DIR/overlay/mnt" 2>/dev/null || true

