拾径 / Backleaf 0.1.0-preview.19

Windows x64 便携测试版：解压整个ZIP，运行Backleaf.exe。或在PowerShell执行：
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex

本批次：
- 文件变化后台准备后原位提交，保留滚动与身份可验证的选择；失败保留旧列表。
- 分区总量/剩余/使用率、卷接口PnP通知和独立后台容量探测。
- 选择浮层增加文件大小；文件夹不递归，未知数据不填0。
- 重复单字母循环定位，输入框及多字母前缀保持正常。
- 29项GPUI无头与90项逻辑/文件/原生检查，OpenPencil v41/247页。

保留路径/分区导航、统一快捷访问和FileView、持久化增量索引、文件操作进度、ZIP创建和ZIP/7z/RAR提取、恢复、多窗口、独立自绘Shell菜单、OLE拖放、虚拟网格/长文本/流光与框选加速。跨查询结果缓存未实现。

保留整个目录、lib与licenses。微软签名相邻运行库和7-Zip引擎均附来源/许可证。用户数据%LOCALAPPDATA%\Backleaf\operations，升级保留，升级前关闭应用。真实USB插拔、故障驱动、IME、NVDA、混合DPI/拔屏、Explorer完整手势及长期压力仍需各自实机证据；无头字体使用合成度量。详见docs/native-test-release.md和docs/storage-v41.md。
