# rfuse3 示例项目

## 📁 项目结构

```
project/test/
├── minimal_rfuse3_example.rs    # ✅ 可编译的最小化示例
├── simple_working_test.sh       # ✅ 简单测试脚本
├── working_test.sh              # ✅ 完整测试脚本
├── debug_test.sh                # ✅ 调试测试脚本
├── Cargo.toml                   # ✅ 项目配置
└── README.md                    # ✅ 说明文档
```

## 🚀 快速开始

### 1. 构建示例

```bash
cd /Users/mac/Desktop/github/rk8s/project/test
cargo build --bin minimal_rfuse3_example
```

### 2. 运行测试

```bash
# 简单测试
./simple_working_test.sh

# 完整测试
./working_test.sh

# 调试测试
./debug_test.sh
```

### 3. 手动测试

```bash
# 创建挂载点
mkdir -p /tmp/minimal_rfuse3_test

# 启动文件系统
./target/debug/minimal_rfuse3_example --mountpoint /tmp/minimal_rfuse3_test

# 在另一个终端测试
ls -la /tmp/minimal_rfuse3_test
cat /tmp/minimal_rfuse3_test/hello.txt
```

## 📋 功能特性

### ✅ 已实现功能

- **只读文件系统** - 提供基本的文件系统功能
- **单文件支持** - 包含一个 `hello.txt` 文件
- **目录列表** - 支持 `ls` 命令
- **文件读取** - 支持 `cat` 命令
- **文件属性** - 支持 `stat` 命令
- **macOS 非特权挂载** - 利用 macOS 的特性

### 📝 示例文件内容

```
Hello, rfuse3! 这是一个最小化的文件系统示例。
```

## 🔧 技术细节

### 依赖项

- `rfuse3` - FUSE 文件系统库
- `tokio` - 异步运行时
- `clap` - 命令行参数解析
- `tracing` - 日志记录

### 实现的方法

- `init()` - 初始化文件系统
- `destroy()` - 销毁文件系统
- `lookup()` - 查找文件
- `getattr()` - 获取文件属性
- `readdir()` - 读取目录
- `open()` - 打开文件
- `read()` - 读取文件内容

## ⚠️ 已知问题

### macOS FUSE 设备问题

在某些情况下，可能会遇到 "Device not configured (os error 6)" 错误。这通常是由于：

1. **FUSE 设备状态异常** - 需要重启 FUSE 服务
2. **权限问题** - 检查用户权限
3. **系统状态** - 可能需要重启系统

### 解决方案

```bash
# 重启 FUSE 服务 (需要管理员权限)
sudo launchctl unload /Library/LaunchDaemons/com.macfuse.filesystems.macfuse.plist
sudo launchctl load /Library/LaunchDaemons/com.macfuse.filesystems.macfuse.plist

# 或者重启系统
sudo reboot
```

## 🎯 使用场景

这个示例适用于：

- **学习 FUSE 文件系统开发**
- **理解 rfuse3 库的使用**
- **作为更复杂文件系统的基础**
- **测试 FUSE 功能**

## 📚 扩展建议

### 添加更多功能

1. **写入支持** - 实现 `write()` 方法
2. **文件创建** - 实现 `create()` 方法
3. **目录创建** - 实现 `mkdir()` 方法
4. **文件删除** - 实现 `unlink()` 方法
5. **多文件支持** - 支持多个文件

### 改进错误处理

1. **更详细的错误信息**
2. **更好的日志记录**
3. **优雅的错误恢复**

## 🤝 贡献

欢迎提交 Issue 和 Pull Request 来改进这个示例！

## 📄 许可证

MIT License
