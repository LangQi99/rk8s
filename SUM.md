macOS 没有 O_PATH 这个概念，你打开文件就必须有读或写的权限。

macOS 没有 /proc/self/fd 这个目录。

fstatat64 是一个获取文件元数据（比如大小、创建时间等）的函数 AT_EMPTY_PATH参数允许fd
macOS 也有类似的函数 (fstatat)，但它不支持 AT_EMPTY_PATH 这个参数

name_to_handle_at / open_by_handle_at 给一个文件生成一个永久且唯一的“句柄” race-free
macOS 完全没有类似的东西

macOS 没有内核级的 overlayfs
macOS 的 xattr 不支持 trusted. 这个命名空间 （比如 trusted.overlay.opaque="y"）