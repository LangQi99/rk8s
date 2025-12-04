#!/bin/bash
# Test script for passthrough filesystem with bind mounts

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/examples/passthrough"

# Test directories
TEST_ROOT="/tmp/bind_passthrough_test_$$"
ROOT_DIR="$TEST_ROOT/root"
MOUNT_POINT="$TEST_ROOT/mount"
HOST_DIR="$TEST_ROOT/host_data"

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    
    # Unmount if still mounted
    if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
        sudo umount "$MOUNT_POINT" 2>/dev/null || true
    fi
    
    # Kill the passthrough process if still running
    if [ -n "$FS_PID" ] && ps -p "$FS_PID" > /dev/null 2>&1; then
        kill "$FS_PID" 2>/dev/null || true
        sleep 1
    fi
    
    # Clean bind mounts under root_dir
    for mount_path in "$ROOT_DIR/bind_test" "$ROOT_DIR/bind_test2"; do
        if mountpoint -q "$mount_path" 2>/dev/null; then
            sudo umount "$mount_path" 2>/dev/null || true
        fi
    done
    
    # Remove test directories
    rm -rf "$TEST_ROOT" 2>/dev/null || true
    
    echo -e "${GREEN}Cleanup completed${NC}"
}

trap cleanup EXIT INT TERM

echo -e "${GREEN}=== Bind Mount Passthrough Test ===${NC}"

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo -e "${RED}Error: Binary not found at $BINARY${NC}"
    echo "Please build the project first: cargo build -p libfuse-fs --examples"
    exit 1
fi

# Create test directories
echo "Creating test directories..."
mkdir -p "$ROOT_DIR"
mkdir -p "$MOUNT_POINT"
mkdir -p "$HOST_DIR"

# Create test content in host directory
echo "test_content_from_host" > "$HOST_DIR/test_file.txt"
mkdir -p "$HOST_DIR/subdir"
echo "nested_content" > "$HOST_DIR/subdir/nested.txt"

# Create some content in root_dir
echo "root_content" > "$ROOT_DIR/root_file.txt"

# Start passthrough filesystem with bind mount
echo "Starting passthrough filesystem with bind mount..."
sudo "$BINARY" \
    --rootdir "$ROOT_DIR" \
    --mountpoint "$MOUNT_POINT" \
    --bind "bind_test:$HOST_DIR" \
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
        echo -e "${RED}Passthrough process died${NC}"
        exit 1
    fi
    sleep 0.5
done

if ! mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
    echo -e "${RED}Failed to mount filesystem${NC}"
    exit 1
fi

# Test 1: Check if bind mount directory exists
echo -e "\n${YELLOW}Test 1: Check bind mount directory${NC}"
if [ -d "$MOUNT_POINT/bind_test" ]; then
    echo -e "${GREEN}✓ Bind mount directory exists${NC}"
else
    echo -e "${RED}✗ Bind mount directory not found${NC}"
    exit 1
fi

# Test 2: Check if host content is accessible
echo -e "\n${YELLOW}Test 2: Read host content through bind mount${NC}"
if [ -f "$MOUNT_POINT/bind_test/test_file.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/bind_test/test_file.txt")
    if [ "$CONTENT" = "test_content_from_host" ]; then
        echo -e "${GREEN}✓ Successfully read host content: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Content mismatch: $CONTENT${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ File not found in bind mount${NC}"
    exit 1
fi

# Test 3: Check nested directory
echo -e "\n${YELLOW}Test 3: Access nested directory in bind mount${NC}"
if [ -f "$MOUNT_POINT/bind_test/subdir/nested.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/bind_test/subdir/nested.txt")
    if [ "$CONTENT" = "nested_content" ]; then
        echo -e "${GREEN}✓ Successfully read nested content: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Nested content mismatch: $CONTENT${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ Nested file not found${NC}"
    exit 1
fi

# Test 4: Write to bind mount (if writable)
echo -e "\n${YELLOW}Test 4: Write to bind mount${NC}"
if echo "new_content" > "$MOUNT_POINT/bind_test/write_test.txt" 2>/dev/null; then
    if [ -f "$HOST_DIR/write_test.txt" ]; then
        CONTENT=$(cat "$HOST_DIR/write_test.txt")
        if [ "$CONTENT" = "new_content" ]; then
            echo -e "${GREEN}✓ Successfully wrote to bind mount${NC}"
        else
            echo -e "${RED}✗ Write content mismatch${NC}"
            exit 1
        fi
    else
        echo -e "${RED}✗ Write did not reach host directory${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ Write test skipped (may be read-only)${NC}"
fi

# Test 5: Check that root_dir content is also accessible
echo -e "\n${YELLOW}Test 5: Access root_dir content${NC}"
if [ -f "$MOUNT_POINT/root_file.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/root_file.txt")
    if [ "$CONTENT" = "root_content" ]; then
        echo -e "${GREEN}✓ Root content accessible: $CONTENT${NC}"
    else
        echo -e "${RED}✗ Root content mismatch${NC}"
        exit 1
    fi
else
    echo -e "${RED}✗ Root file not found${NC}"
    exit 1
fi

echo -e "\n${GREEN}=== All tests passed! ===${NC}"
exit 0
