拾径 / Backleaf 0.1.0-preview.16

Windows x64 便携测试版：解压整个ZIP，运行Backleaf.exe。也可在PowerShell执行：
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex

本批次：
- 父菜单尺寸固定，应用操作由固定入口展开；每级子菜单独立owned窗口，就绪后出现。
- 子菜单打开/关闭及第三方异步返回不再调整父窗口尺寸。
- 20项GPUI无头交互与82项逻辑/文件/原生协议/隐藏Win32测试，分别门禁。
- ZIP携带经过微软签名与摘要核验的相邻VC++运行库，不用额外安装或修改全局PATH。
- OpenPencil v39/237页，保留全部历史基线。

保留路径编辑和分区导航、统一快捷访问、搜索及持久化增量元数据索引、文件操作与进度、ZIP创建及ZIP/7z/RAR提取、目录变更检测、恢复和独立窗口、OLE拖放及自绘扩展菜单、虚拟网格/长文本/流光与框选加速。

文件区CRUD刷新抖动、容量/选区大小、移动介质事件更新与重复字母循环仍在后续事项；跨查询结果缓存尚未实现。

用户数据：%LOCALAPPDATA%\Backleaf\operations。升级保留数据，升级前关闭应用。原生IME、UIA/NVDA、混合DPI/拔屏、Explorer完整跨应用鼠标手势及长期压力仍有未验证边界；无头字体为合成度量，不把旧EXE截图/性能冒充本版。最终可见窗口回归按具体版本记录。

保留整个目录、lib与licenses。包内有EXE、Logo、237页可编辑设计、build.json、headless.json、VC++运行库声明、7-Zip DLL及对应完整源包。详情见仓库docs/native-test-release.md和docs/menu-stability-v39.md。
