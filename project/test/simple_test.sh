# 1. 创建挂载点
mkdir -p /tmp/minimal_rfuse3_test

# 2. 运行文件系统
cd /Users/mac/Desktop/github/rk8s/project/test
./target/debug/minimal_rfuse3_example --mountpoint /tmp/minimal_rfuse3_test

# 3. 在另一个终端测试
ls -la /tmp/minimal_rfuse3_test
cat /tmp/minimal_rfuse3_test/hello.txt

# 4. 停止文件系统 (Ctrl+C 或 pkill)