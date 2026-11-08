拾径 / Backleaf 0.1.0

Windows 原生文件管理器，提供 x64 与 ARM64 包。
解压对应 ZIP，保留 DLL、lib 和许可证目录，运行 Backleaf.exe。

安装或升级，在 PowerShell 执行：
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex

安装器优先选择最新正式版，按 Windows 进程架构下载对应包；升级前关闭 Backleaf。
安装无需管理员权限，运行库位于应用目录，不修改系统 PATH。
安装目录：%LOCALAPPDATA%\Programs\Backleaf\current
用户数据：%LOCALAPPDATA%\Backleaf\operations

支持目录网格与多窗口、复制/移动/回收站删除、归档、搜索与保存搜索、文本内容搜索、图片缩略图、文件属性和状态恢复。
界面支持中文 / English，Ctrl+Shift+L 切换。
应用图标采用黄色文件夹与圆叶表情。

已知范围：内容搜索为有上限的文本扫描，不是持久全文索引；HEIC/AVIF/视频缩略图暂不支持。
第三方 Shell 菜单兼容性取决于扩展。ARM64 已在 hosted runner 验证，尚无长期实体设备验证。
混合 DPI、屏幕阅读器与长时间操作压力的完整兼容性验证仍在继续。

发布与来源：https://github.com/simiooo/backleaf/releases/tag/v0.1.0
构建来源、摘要和检查记录见包内 build.json / build-arm64.json 与 headless.json。
本包包含 LICENSE、依赖及素材许可证、OpenPencil 设计文件与 Logo 生成记录。
