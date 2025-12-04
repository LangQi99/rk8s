#!/bin/bash
# Test script for overlay filesystem with bind mounts

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/examples/overlayfs_example"

# Test directories
TEST_ROOT="/tmp/bind_overlay_test_$$"
UPPER_DIR="$TEST_ROOT/upper"
LOWER_DIR="$TEST_ROOT/lower"
MOUNT_POINT="$TEST_ROOT/merged"
HOST_DIR1="$TEST_ROOT/host_proc"
HOST_DIR2="$TEST_ROOT/host_sys"

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    
    # Unmount if still mounted
    if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
        sudo umount "$MOUNT_POINT" 2>/dev/null || true
    fi
    
    # Kill the overlayfs process if still running
    if [ -n "$FS_PID" ] && ps -p "$FS_PID" > /dev/null 2>&1; then
        kill "$FS_PID" 2>/dev/null || true
        sleep 1
    fi
    
    # Clean bind mounts under upper_dir
    for mount_path in "$UPPER_DIR/proc_bind" "$UPPER_DIR/sys_bind"; do
        if mountpoint -q "$mount_path" 2>/dev/null; then
            sudo umount "$mount_path" 2>/dev/null || true
        fi
    done
    
    # Remove test directories
    rm -rf "$TEST_ROOT" 2>/dev/null || true
    
    echo -e "${GREEN}Cleanup completed${NC}"
}

trap cleanup EXIT INT TERM

echo -e "${GREEN}=== Bind Mount Overlay Test ===${NC}"

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo -e "${RED}Error: Binary not found at $BINARY${NC}"
    echo "Please build the project first: cargo build -p libfuse-fs --examples"
    exit 1
fi

# Create test directories
echo "Creating test directories..."
mkdir -p "$UPPER_DIR"
mkdir -p "$LOWER_DIR"
mkdir -p "$MOUNT_POINT"
mkdir -p "$HOST_DIR1"
mkdir -p "$HOST_DIR2"

# Create test content in host directories
echo "host_proc_content" > "$HOST_DIR1/proc_test.txt"
echo "host_sys_content" > "$HOST_DIR2/sys_test.txt"

# Create test content in layers
echo "upper_content" > "$UPPER_DIR/upper_file.txt"
echo "lower_content" > "$LOWER_DIR/lower_file.txt"

# Start overlay filesystem with bind mounts
echo "Starting overlay filesystem with bind mounts..."
sudo "$BINARY" \
    --mountpoint "$MOUNT_POINT" \
    --upperdir "$UPPER_DIR" \
    --lowerdir "$LOWER_DIR" \
    --bind "proc_bind:$HOST_DIR1" \
    --bind "sys_bind:$HOST_DIR2" \
    --privileged \
    --allow-other &
FS_PID=$!

# Wait for filesystem to be ready
echo "Waiting for filesystem to mount..."
for i in {1..30}; do
    if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
        echo -e "${GREEN}Filesystem mounted successfully${NC}"
        break
    fi
    if ! ps -p "$FS_PID" > /dev/null 2>&1; then
        echo -e "${RED}Overlay process died${NC}"
        exit 1
    fi
    sleep 0.5
done

if ! mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
    echo -e "${RED}Failed to mount filesystem${NC}"
    exit 1
fi

# Test 1: Check if bind mount directories exist
echo -e "\n${YELLOW}Test 1: Check bind mount directories${NC}"
if [ -d "$MOUNT_POINT/proc_bind" ] && [ -d "$MOUNT_POINT/sys_bind" ]; then
    echo -e "${GREEN}✓ Both bind mount directories exist${NC}"
else
    echo -e "${RED}✗ Bind mount directories not found${NC}"
    exit 1
fi

# Test 2: Check if host content is accessible from first bind mount
echo -e "\n${YELLOW}Test 2: Read first bind mount content${NC}"
if [ -f "$MOUNT_POINT/proc_bind/proc_test.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/proc_bind/proc_test.txt")
    if [ "$CONTENT" = "host_proc_content" ]; then
        echo -e "${GREEN}✓ Successfully read proc bind mount: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Content mismatch: $CONTENT${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ File not found in proc bind mount${NC}"
    exit 1
fi

# Test 3: Check if host content is accessible from second bind mount
echo -e "\n${YELLOW}Test 3: Read second bind mount content${NC}"
if [ -f "$MOUNT_POINT/sys_bind/sys_test.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/sys_bind/sys_test.txt")
    if [ "$CONTENT" = "host_sys_content" ]; then
        echo -e "${GREEN}✓ Successfully read sys bind mount: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Content mismatch: $CONTENT${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ File not found in sys bind mount${NC}"
    exit 1
fi

# Test 4: Check overlay functionality (upper layer)
echo -e "\n${YELLOW}Test 4: Access upper layer content${NC}"
if [ -f "$MOUNT_POINT/upper_file.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/upper_file.txt")
    if [ "$CONTENT" = "upper_content" ]; then
        echo -e "${GREEN}✓ Upper layer accessible: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Upper content mismatch${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ Upper file not found${NC}"
    exit 1
fi

# Test 5: Check overlay functionality (lower layer)
echo -e "\n${YELLOW}Test 5: Access lower layer content${NC}"
if [ -f "$MOUNT_POINT/lower_file.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/lower_file.txt")
    if [ "$CONTENT" = "lower_content" ]; then
        echo -e "${GREEN}✓ Lower layer accessible: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Lower content mismatch${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ Lower file not found${NC}"
    exit 1
fi

# Test 6: Write new file and verify it appears in both merged and upper
echo -e "\n${YELLOW}Test 6: Write through overlay${NC}"
if echo "overlay_write" > "$MOUNT_POINT/new_file.txt" 2>/dev/null; then
    if [ -f "$UPPER_DIR/new_file.txt" ]; then
        CONTENT=$(cat "$UPPER_DIR/new_file.txt")
        if [ "$CONTENT" = "overlay_write" ]; then
            echo -e "${GREEN}✓ Overlay write successful${NC}"
        else
            echo -e "${RED}✗ Write content mismatch${NC}"
            exit 1
        fi
    else
        echo -e "${RED}✗ Write did not reach upper directory${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ Failed to write to overlay${NC}"
    exit 1
fi

# Test 7: Verify bind mounts and overlay coexist correctly
echo -e "\n${YELLOW}Test 7: List merged directory${NC}"
ls -la "$MOUNT_POINT" > /tmp/overlay_ls.txt 2>&1 || true
if grep -q "proc_bind" /tmp/overlay_ls.txt && grep -q "sys_bind" /tmp/overlay_ls.txt; then
    echo -e "${GREEN}✓ Both bind mounts visible in merged directory${NC}"
else
    echo -e "${RED}✗ Bind mounts not properly listed${NC}"
    cat /tmp/overlay_ls.txt
    exit 1
fi

echo -e "\n${GREEN}=== All tests passed! ===${NC}"
exit 0
