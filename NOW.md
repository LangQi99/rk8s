rfuse3 libfuse-fs 的 macOS 适配情况
已实现的部分
rfuse3 已经实现了对 macOS 的基本支持：

macFUSE 挂载支持：通过 find_macfuse_mount() 函数查找并使用 macFUSE 的挂载工具 lib.rs:243-255

macOS 特定的文件属性：支持 macOS 特有的时间戳字段（创建时间 crtime）和文件标志 flags abi.rs:236-260

macOS 特定的 SetAttr 字段：包括 crtime、chgtime、bkuptime 和 flags lib.rs:119-127

macOS 特定的能力标志：在初始化时支持协商 FUSE_ALLOCATE、FUSE_EXCHANGE_DATA、FUSE_CASE_INSENSITIVE、FUSE_VOL_RENAME 和 FUSE_XTIMES 等 macOS 专有标志 abi.rs:164-173

macOS 连接模式：支持通过 macFUSE 的 socket 通信方式进行非特权挂载

尚未实现的部分
三个 macOS 特定操作码的实现为空：虽然定义了 FUSE_SETVOLNAME、FUSE_GETXTIMES 和 FUSE_EXCHANGE 这三个 macOS 专有的操作码，但它们的处理函数目前都是空实现 session.rs:3740-3747
这三个操作码的数据结构已定义： abi.rs:549-609

ioctl 操作未实现：这是 rfuse3 明确声明不支持的功能，在 README 中有说明 README.md:22-25

一些 Linux 特定的功能在 macOS 上被禁用：如 FUSE_SPLICE_READ、FUSE_SPLICE_WRITE 和 FUSE_SPLICE_MOVE abi.rs:95-105

Notes
rfuse3 对 macOS 的基础支持已经比较完善，可以正常挂载和使用文件系统。主要缺失的是三个 macOS 特定操作码的具体实现：

FUSE_SETVOLNAME (opcode 61)：用于设置卷名
FUSE_GETXTIMES (opcode 62)：用于获取扩展时间信息（备份时间、创建时间等）
FUSE_EXCHANGE (opcode 63)：用于交换文件数据
这些操作码虽然已在协议层定义，但实际的业务逻辑处理是空的。如果需要完整的 macOS 功能支持，需要实现这些操作码对应的文件系统接口方法。
