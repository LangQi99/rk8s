# Bind Mount Support in libfuse-fs

## Overview

This document describes the bind mount feature added to `libfuse-fs` that enables container volume management without requiring kernel mount privileges for each operation.

## Features

- **User-space bind mount management**: Bind mounts are configured through passthrough and overlay filesystem APIs
- **Container volume support**: Enables mounting host directories (e.g., `/proc`, `/sys`, `/dev`) into overlay merged layers
- **Multiple bind mounts**: Support for multiple bind mount points simultaneously
- **Read/Write support**: Bind mounts can be configured as read-only or writable
- **Automatic cleanup**: Bind mounts are cleaned up when the filesystem unmounts

## Usage

### Command Line Interface

Both `passthrough` and `overlayfs_example` binaries support the `--bind` flag:

```bash
--bind "target:source"
```

Where:
- `target`: Path relative to the root directory where the bind mount will appear
- `source`: Absolute path on the host to bind mount

Multiple `--bind` flags can be specified for multiple bind mounts.

### Examples

#### Passthrough Filesystem with Bind Mount

```bash
sudo ./target/debug/examples/passthrough \
    --rootdir /path/to/root \
    --mountpoint /path/to/mount \
    --bind "proc:/proc" \
    --bind "sys:/sys" \
    --privileged \
    --allow-other
```

#### Overlay Filesystem with Bind Mounts

```bash
sudo ./target/debug/examples/overlayfs_example \
    --mountpoint /root/merged \
    --upperdir /root/upper \
    --lowerdir /root/ubuntu-rootfs \
    --bind "proc:/proc" \
    --bind "sys:/sys" \
    --bind "dev:/dev" \
    --bind "dev/pts:/dev/pts" \
    --bind "etc/resolv.conf:/etc/resolv.conf" \
    --privileged \
    --allow-other
```

### Programmatic API

#### PassthroughArgs

```rust
use libfuse_fs::passthrough::PassthroughArgs;

let args = PassthroughArgs {
    root_dir: "/path/to/root",
    mapping: None,
    bind_mounts: vec![
        ("proc".to_string(), "/proc".to_string()),
        ("sys".to_string(), "/sys".to_string()),
    ],
};

let fs = new_passthroughfs_layer(args).await?;
```

#### OverlayArgs

```rust
use libfuse_fs::overlayfs::{OverlayArgs, mount_fs};

let bind_mounts = vec![
    ("proc".to_string(), "/proc".to_string()),
    ("sys".to_string(), "/sys".to_string()),
];

let mount_handle = mount_fs(OverlayArgs {
    mountpoint: "/path/to/mount",
    upperdir: "/path/to/upper",
    lowerdir: vec!["/path/to/lower"],
    privileged: true,
    mapping: None,
    name: None::<String>,
    allow_other: true,
    bind_mounts,
}).await;
```

## Implementation Details

### Architecture

1. **Configuration**: Bind mounts are specified in `PassthroughArgs` and stored in `Config`
2. **Setup**: During `PassthroughFs::import()`, bind mounts are created using kernel `mount --bind`
3. **Operation**: FUSE operations transparently access bind-mounted directories
4. **Cleanup**: Bind mounts are tracked and can be explicitly cleaned up via `cleanup_bind_mounts()`

### Key Components

- `passthrough/config.rs`: Stores bind mount configuration
- `passthrough/mod.rs`: Implements bind mount setup and cleanup
- `overlayfs/mod.rs`: Propagates bind mounts to upper layer

### Security Considerations

- Requires `sudo` privileges to execute kernel bind mount commands
- Bind mounts persist as long as the filesystem is mounted
- Cleanup is performed explicitly or on process termination
- Always validate paths to prevent security issues

## Testing

Two test scripts are provided:

### bind_passthrough_test.sh

Tests bind mount functionality with passthrough filesystem:
```bash
cd project/libfuse-fs
./tests/bind_passthrough_test.sh
```

### bind_overlay_test.sh

Tests bind mount functionality with overlay filesystem:
```bash
cd project/libfuse-fs
./tests/bind_overlay_test.sh
```

Both tests verify:
- Bind mount directory creation
- Read access to bind-mounted content
- Write access (if writable)
- Multiple bind mounts coexisting
- Overlay layer functionality

## Troubleshooting

### "Transport endpoint is not connected"

This error typically indicates a stale mount point. Clean it up:

```bash
sudo umount -l /path/to/mount
sudo rm -rf /path/to/mount
sudo mkdir -p /path/to/mount
```

### Permission Denied

Ensure the command is run with `sudo` or appropriate privileges:

```bash
sudo ./target/debug/examples/overlayfs_example ...
```

### Bind Mount Not Visible

Check that:
1. The source path exists and is accessible
2. The target directory was created successfully
3. The bind mount was actually performed (check with `mountpoint`)

```bash
mountpoint /path/to/upper/proc
ls -la /path/to/upper/proc
```

## Future Enhancements

Potential improvements for the bind mount feature:

1. **Read-only flag**: Add support for read-only bind mounts
2. **Recursive bind mounts**: Support for `--rbind` option
3. **Unmount on error**: Automatic cleanup on initialization errors
4. **User-space implementation**: Implement bind mount logic without kernel mount for rootless containers
5. **Bind mount options**: Support additional mount options (noexec, nosuid, etc.)

## References

- Original issue: Container volume management requirements
- Linux `mount(2)` man page
- FUSE documentation
- Kubernetes CSI specification
