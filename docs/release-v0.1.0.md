# Backleaf 0.1.0

拾径 / Backleaf 首个正式发布版，支持 Windows x64 与 ARM64，采用黄色文件夹与圆叶表情图标。

- 原生虚拟文件网格、多窗口与状态恢复。
- 文件复制、移动、回收站删除、归档、失败项重试与文件属性。
- 文件名/元数据索引搜索、保存搜索与文本内容搜索。
- 按需图片缩略图、中文 / English 界面。
- 当前用户安装、app-local 运行库和资源管理器“在 Backleaf 中打开”入口。

下载对应架构 ZIP，解压运行 `Backleaf.exe`；升级前关闭旧版本。也可使用安装命令：

```powershell
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex
```

安装器默认优先正式版，保留通过 `-Tag` 指定预览版的能力。正式 Release 在 x64、ARM64 构建、回归、包内 EXE 及真实安装器冒烟通过后一次性公开；提供源码提交、构建记录和 SHA-256 摘要。

已知范围：内容搜索是有上限的文本扫描，暂不提供持久全文索引、正则或旧编码支持；HEIC/AVIF/视频缩略图不支持。第三方 Shell 扩展兼容性因软件而异。ARM64 执行验证来自 hosted runner，混合 DPI、屏幕阅读器、实体 ARM 设备及长时间压力的完整验证仍在继续。

preview.52 的 ARM64 长路径属性窗口无头用例曾一次失败、完整重跑通过，根因尚未确定；正式发布流程分别保留原始日志和 JSON 报告，继续执行完整门禁。
