# Manual Testing Instructions for DevPTS Fix

## Prerequisites

You need a **fresh Linux environment** with:
- Docker installed
- Rust toolchain (cargo)
- Root access (sudo)
- The rk8s repository cloned

## Quick Test (5 minutes)

This is the minimal test to verify the fix works.

### 1. Setup Test Environment

```bash
# Clone and enter repository
cd /path/to/rk8s

# Create test directory
mkdir -p /tmp/test-devpts
cd /tmp/test-devpts

# Get Ubuntu rootfs
docker pull ubuntu:22.04
docker create --name ubuntu-export ubuntu:22.04
docker export ubuntu-export -o ubuntu-rootfs.tar
docker rm ubuntu-export
mkdir -p ubuntu-rootfs upper merged
tar -xf ubuntu-rootfs.tar -C ubuntu-rootfs/
```

### 2. Build Binary

```bash
cd /path/to/rk8s/project
cargo build -p libfuse-fs --example overlayfs_example
```

### 3. Run Test

```bash
cd /tmp/test-devpts

# Start overlayfs with bind mounts
sudo /path/to/rk8s/project/target/debug/examples/overlayfs_example \
    --mountpoint /tmp/test-devpts/merged \
    --upperdir /tmp/test-devpts/upper \
    --lowerdir /tmp/test-devpts/ubuntu-rootfs \
    --bind "proc:/proc" \
    --bind "sys:/sys" \
    --bind "dev:/dev" \
    --bind "dev/pts:/dev/pts" \
    --privileged &

MOUNT_PID=$!
sleep 3

# Verify it works
sudo chroot /tmp/test-devpts/merged /bin/bash -c "ls /dev/pts"

# Stop it
sudo kill -INT $MOUNT_PID
sleep 3
```

### 4. Verify Fix

**Critical Test - This is what failed before the fix:**

```bash
# Check host devpts is still mounted
mount | grep "^devpts on /dev/pts"
# Expected: devpts on /dev/pts type devpts (rw,...)

# Check PTY allocation works
ls /dev/pts/
# Expected: List of PTY devices (0, 1, 2, ...)

# Check sudo works
sudo echo "test"
# Expected: "test" printed, no PTY errors
```

**If any of these fail, the fix didn't work!**

### 5. Run Multiple Cycles

```bash
for i in {1..5}; do
    echo "Cycle $i"
    
    sudo /path/to/rk8s/project/target/debug/examples/overlayfs_example \
        --mountpoint /tmp/test-devpts/merged \
        --upperdir /tmp/test-devpts/upper \
        --lowerdir /tmp/test-devpts/ubuntu-rootfs \
        --bind "dev:/dev" \
        --bind "dev/pts:/dev/pts" \
        --privileged &
    
    PID=$!
    sleep 2
    sudo kill -INT $PID
    sleep 2
    
    # Verify PTY still works
    if ! sudo echo "Cycle $i OK" > /dev/null 2>&1; then
        echo "FAILED: PTY broken after cycle $i"
        exit 1
    fi
done

echo "SUCCESS: All 5 cycles passed!"
```

## What Should Happen

### Before Fix (Old Behavior)
1. ❌ After unmount: `mount | grep devpts` shows no devpts on `/dev/pts`
2. ❌ `ls /dev/pts/` fails or shows empty
3. ❌ `sudo echo test` fails with: `sudo: unable to allocate pty: No such device`
4. ❌ New bash sessions fail with: `forkpty(3) failed`
5. ❌ Must manually run: `sudo mount -t devpts devpts /dev/pts` to recover

### After Fix (Expected Behavior)
1. ✅ After unmount: `mount | grep devpts` shows devpts still on `/dev/pts`
2. ✅ `ls /dev/pts/` works and shows PTY devices
3. ✅ `sudo echo test` works normally
4. ✅ New bash sessions work normally
5. ✅ No manual recovery needed

## Troubleshooting

### If Tests Fail

**Environment is already broken:**
```bash
# Check if devpts is mounted
mount | grep devpts

# If not mounted, recover with:
sudo mount -t devpts devpts /dev/pts

# Then try tests again
```

**Compilation errors:**
```bash
cd /path/to/rk8s/project
cargo clean
cargo build -p libfuse-fs --example overlayfs_example
```

**Permission denied:**
```bash
# All mount operations require root
sudo -i
# Then run commands from root shell
```

### Observing the Fix in Action

You can watch the unmount behavior with strace:

```bash
# In terminal 1, start the mount
sudo strace -f -e trace=umount,umount2 \
    /path/to/rk8s/project/target/debug/examples/overlayfs_example \
    --mountpoint /tmp/test-devpts/merged \
    --upperdir /tmp/test-devpts/upper \
    --lowerdir /tmp/test-devpts/ubuntu-rootfs \
    --bind "dev/pts:/dev/pts" \
    --privileged 2>&1 | grep -i umount

# In terminal 2, after it starts, send SIGINT
sudo killall -INT overlayfs_example
```

**Expected output with fix:**
- Should see `umount("/tmp/test-devpts/merged/dev/pts")` (no MNT_DETACH flag)
- Should NOT see `umount("/dev/pts")`

**Old behavior (buggy):**
- Would see `umount2("/tmp/test-devpts/merged/dev/pts", MNT_DETACH)`
- Might affect `/dev/pts` on host

## Complete Test Output Example

```
$ sudo /path/to/rk8s/project/target/debug/examples/overlayfs_example ... &
[1] 12345

$ sudo chroot /tmp/test-devpts/merged /bin/bash -c "ls /dev/pts"
0  1  2  3  ptmx

$ sudo kill -INT 12345
[1]+ Done

$ mount | grep "^devpts on /dev/pts"
devpts on /dev/pts type devpts (rw,nosuid,noexec,relatime,gid=5,mode=620,ptmxmode=000)

$ sudo echo "test"
test

$ echo "✅ Fix verified!"
✅ Fix verified!
```

## CI/CD Integration

To integrate this into CI/CD:

```yaml
- name: Test devpts fix
  run: |
    # Setup
    mkdir -p /tmp/test-devpts
    cd /tmp/test-devpts
    docker pull ubuntu:22.04
    docker create --name ubuntu-export ubuntu:22.04
    docker export ubuntu-export -o ubuntu-rootfs.tar
    docker rm ubuntu-export
    mkdir -p ubuntu-rootfs upper merged
    tar -xf ubuntu-rootfs.tar -C ubuntu-rootfs/
    
    # Build
    cd $GITHUB_WORKSPACE/project
    cargo build -p libfuse-fs --example overlayfs_example
    
    # Test
    cd /tmp/test-devpts
    sudo $GITHUB_WORKSPACE/project/target/debug/examples/overlayfs_example \
        --mountpoint /tmp/test-devpts/merged \
        --upperdir /tmp/test-devpts/upper \
        --lowerdir /tmp/test-devpts/ubuntu-rootfs \
        --bind "dev/pts:/dev/pts" \
        --privileged &
    MOUNT_PID=$!
    sleep 3
    sudo kill -INT $MOUNT_PID
    sleep 3
    
    # Verify
    if ! mount | grep -q "^devpts on /dev/pts"; then
        echo "FAIL: devpts not mounted after unmount"
        exit 1
    fi
    
    if ! sudo echo "test" > /dev/null 2>&1; then
        echo "FAIL: PTY allocation broken"
        exit 1
    fi
    
    echo "✅ DevPTS fix verified"
```

## Summary

The fix changes the unmount behavior from:
- **Before**: Always use `umount2(MNT_DETACH)` → can unmount host `/dev/pts`
- **After**: Use `umount()` first → safe, only unmounts the bind mount

This simple change prevents the host system's critical mounts from being affected.
