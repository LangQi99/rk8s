mkdir -p /tmp/vfs
cargo build --example vfs
cargo run --example vfs -- --mountpoint /tmp/vfs &
sleep 2
ls -l /tmp/vfs
cat /tmp/vfs/hello.txt
stat /tmp/vfs
umount /tmp/vfs
