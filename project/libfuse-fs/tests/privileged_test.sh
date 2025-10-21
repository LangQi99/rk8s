sudo -E env CARGO_HOME=$CARGO_HOME RUSTUP_HOME=$RUSTUP_HOME PATH=$PATH bash /home/langqi99/Shared/github/rk8s/project/libfuse-fs/tests/integration_test.sh
sudo umount /home/langqi99/Shared/github/rk8s/project/libfuse-fs/tests/test_artifacts/work/passthrough/mnt
sudo umount /home/langqi99/Shared/github/rk8s/project/libfuse-fs/tests/test_artifacts/work/overlay/mnt 
