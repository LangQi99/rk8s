#!/bin/bash
# OverlayFS + Bind Mount Test
# Verifies that bind mounts work correctly when attached to the upper layer of OverlayFS.

set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
echo_success() { echo -e "${GREEN}[✓]${NC} $1"; }
echo_error() { echo -e "${RED}[✗]${NC} $1"; }
echo_step() { echo -e "\n${YELLOW}━━━ $1 ━━━${NC}"; }

# Check for fusermount
if ! command -v fusermount &> /dev/null; then
    echo_error "fusermount not found"
    exit 1
fi

# Setup directories
TEST_DIR="/tmp/overlay_bind_test_$$"
LOWER_DIR="${TEST_DIR}/lower"
UPPER_DIR="${TEST_DIR}/upper"
WORK_DIR="${TEST_DIR}/work" # Not used by current overlayfs_example but good practice
MNT_DIR="${TEST_DIR}/mnt"
HOST_RW="${TEST_DIR}/host_rw"
HOST_RO="${TEST_DIR}/host_ro"

cleanup() {
    echo_step "Cleanup"
    if mountpoint -q "${MNT_DIR}" 2>/dev/null; then
        echo_info "Unmounting..."
        fusermount -u "${MNT_DIR}" 2>/dev/null || umount "${MNT_DIR}" 2>/dev/null || true
        sleep 1
    fi
    
    if [ -n "${PID}" ] && kill -0 ${PID} 2>/dev/null; then
        echo_info "Killing process ${PID}..."
        kill ${PID} 2>/dev/null || true
        sleep 1
        kill -9 ${PID} 2>/dev/null || true
    fi
    
    if [ -d "${TEST_DIR}" ]; then
        echo_info "Removing test dir..."
        rm -rf "${TEST_DIR}"
    fi
    echo_success "Done"
}

trap cleanup EXIT INT TERM

echo_step "Preparing Environment"
mkdir -p "${LOWER_DIR}" "${UPPER_DIR}" "${MNT_DIR}" "${HOST_RW}" "${HOST_RO}"

# Create files
echo "lower file" > "${LOWER_DIR}/lower.txt"
echo "upper file" > "${UPPER_DIR}/upper.txt"
echo "host rw file" > "${HOST_RW}/rw.txt"
echo "host ro file" > "${HOST_RO}/ro.txt"

echo_success "Environment prepared"

echo_step "Starting OverlayFS with Bind Mounts"
# We expect the binary to support --bind
# Usage: overlayfs_example --mountpoint <mnt> --lowerdir <lower> --upperdir <upper> --bind <mnt>:<host>[:ro]

cd "$(dirname "$0")/.."
echo_info "Building example..."
cargo build --example overlayfs_example --quiet

echo_info "Running overlayfs_example..."
# Note: Assuming we add --bind support to overlayfs_example
cargo run --example overlayfs_example --quiet -- \
    --mountpoint "${MNT_DIR}" \
    --lowerdir "${LOWER_DIR}" \
    --upperdir "${UPPER_DIR}" \
    --bind "bind_rw:${HOST_RW}" \
    --bind "bind_ro:${HOST_RO}:ro" \
    > /tmp/overlay_log_$$.txt 2>&1 &

PID=$!
echo_info "PID: ${PID}"

# Wait for mount
for i in {1..20}; do
    if mountpoint -q "${MNT_DIR}" 2>/dev/null; then
        echo_success "Mounted"
        break
    fi
    if ! kill -0 ${PID} 2>/dev/null; then
        echo_error "Process exited prematurely"
        cat /tmp/overlay_log_$$.txt
        exit 1
    fi
    sleep 0.3
done

sleep 1

echo_step "Testing Basic OverlayFS"
if [ -f "${MNT_DIR}/lower.txt" ] && [ -f "${MNT_DIR}/upper.txt" ]; then
    echo_success "OverlayFS basic files visible"
else
    echo_error "OverlayFS files missing"
    ls -R "${MNT_DIR}"
    exit 1
fi

echo_step "Testing RW Bind Mount"
if [ -f "${MNT_DIR}/bind_rw/rw.txt" ]; then
    echo_success "Bind mount file visible"
else
    echo_error "Bind mount file missing"
    echo_info "Listing MNT_DIR:"
    ls -R "${MNT_DIR}"
    echo_info "Listing UPPER_DIR:"
    ls -R "${UPPER_DIR}"
    echo_info "Overlay Log:"
    cat /tmp/overlay_log_$$.txt
    exit 1
fi

echo_info "Writing to bind mount..."
echo "new data" > "${MNT_DIR}/bind_rw/new.txt"
if [ -f "${HOST_RW}/new.txt" ]; then
    echo_success "Write propagated to host"
else
    echo_error "Write failed to propagate"
    exit 1
fi

echo_step "Testing RO Bind Mount"
if [ -f "${MNT_DIR}/bind_ro/ro.txt" ]; then
    echo_success "RO bind mount file visible"
else
    echo_error "RO bind mount file missing"
    exit 1
fi

echo_info "Attempting write to RO bind mount (should fail)..."
if echo "fail" > "${MNT_DIR}/bind_ro/fail.txt" 2>/dev/null; then
    echo_error "Write succeeded on RO mount!"
    exit 1
else
    echo_success "Write failed as expected"
fi

echo_step "Summary"
echo_success "All tests passed!"
