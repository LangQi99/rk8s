MOUNTPOINT="/tmp/vfs0"
mkdir -p "$MOUNTPOINT"
cargo build --example vfs
cargo run --example vfs -- --mountpoint "$MOUNTPOINT" &
sleep 2

ls "$MOUNTPOINT"
ls -l "$MOUNTPOINT"
ls -al "$MOUNTPOINT"

cat "$MOUNTPOINT/hello.txt"
stat "$MOUNTPOINT"
umount "$MOUNTPOINT"
