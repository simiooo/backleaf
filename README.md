# 拾径 / Backleaf

Windows 原生 GPUI 文件管理器。当前候选版为 0.1.0-preview.42；项目范围跟随 Nuto SIM-919、SIM-934，交付后进入产品验收。

当前可编辑设计稿：[OpenPencil v48 设计草稿](design/Backleaf-v48.fig)，275页，基于v46保留271页并新增Apple式白/黑内容、蓝文件夹与悬浮identity 4页。版本索引见[设计目录](design/README.md)。设计先于实施；旧版design/Backleaf.fig保留为v6历史基线。

已实施原生虚拟文件网格、长文本、统一快捷访问与FileView、搜索和本地元数据索引、文件操作及进度、归档、目录变更监听、路径/查询恢复与独立窗口。本版在后台准备文件变化快照，原位更新并保留滚动及身份可验证的选择；搜索自动刷新也保留旧结果直到新页就绪。独立菜单与相邻运行库沿用preview16。本版补齐分区容量与使用率、PnP介质通知、非递归选中文件大小和重复字母循环定位。本版新增确认失败项重试（只重放已确认失败的顶层项，成功、未确认、取消及部分完成目录不重放），并完成 Explorer 文件剪贴板互操作实机验证。本版修复删除/移动后刷新空档造成的整行空白，以及刷新期间外部拖放被误判为不可放置的问题，应用内操作完成即刷新。本版进一步让拖放/粘贴后的刷新不再等待防抖或窗口激活，目录刷新期间 Ctrl+V 直接粘贴，并把左侧当前项改为低强调中性状态。本版修复点击顶栏时无法收起路径下拉与搜索弹层的问题，并让搜索框和路径下拉按钮保留各自的点击行为。本版让默认搜索在已索引目录内自动使用持久索引，覆盖不足或索引不可用时自动回退实时扫描，不改变结果语义；见[搜索自动使用索引](docs/search-auto-index.md)。本版重构持久索引维护路径（批量目录枚举、keyed 目录 diff、批事务与改名配对），加入空闲调度、按需自动根与 LRU，并让 2 字符查询使用 2-gram 倒排；见[索引维护](docs/index-maintenance.md)。本版修复空查询仍保留搜索浮层的问题：空输入只保留输入框，清除已输入的查询会退出搜索并恢复文件区焦点。本版同时调快框选边缘自动滚动，使其更接近资源管理器的跟手速度；路径面包屑改为按实际可用宽度动态展开，不再过早显示省略号。本版还确保高级搜索/筛选项不会在空查询时启动整树扫描；已完成且不可重试的文件操作结果不再常驻顶部任务入口和底部结果面板。本版为图片文件在可视区内按需解码缩略图，替换文件图标并按比例居中填满单元格且不溢出；修复选中底板未包住缩略图导致的视觉溢出，并合并解码通知以减少重复重渲染。最小化/失焦/重新聚焦不再因记账刷新清空缩略图缓存；只有文件监听内容变化才重新解码。高级搜索容量字段支持 10MB/500KB/1.5GB 等宽容单位输入，非法值会在对应输入框红框并在下方显示错误。Shell 异步次级菜单为空时显示“没有可用的菜单项”空状态，不再出现空白半截面板。本版新增“内容”搜索模式：递归检查文本文件内容，用响应式小卡片展示上下文并高亮匹配，不再只搜文件名。本版增加中文/English 界面：按系统语言自动选择，Ctrl+Shift+L 切换并记住；主界面、菜单、搜索/内容搜索、筛选、文件操作、详情、侧栏与常见错误已翻译，少量未覆盖内部错误回退中文。实测范围和限制见[测试版说明](docs/native-test-release.md)。

Windows PowerShell 5.1 / PowerShell 7，一行安装最新测试版，无需管理员权限：

```powershell
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex
```

从开始菜单打开 Backleaf。重复运行同一命令即可升级，升级前请关闭 Backleaf。安装目录为`%LOCALAPPDATA%\Programs\Backleaf\current`，用户数据仍保存在`%LOCALAPPDATA%\Backleaf`。脚本自动补齐应用目录内的VC++运行库，校验 GitHub 资产 SHA256、包内 EXE/归档库及版本来源，失败不替换已安装版本。无需配置环境变量；不修改全局PATH。[安装参数与验证](docs/install.md)。

Windows/MSVC/Rust 1.99.1：

```powershell
./scripts/native/bootstrap.ps1
./scripts/native/check.ps1
./target/release/backleaf.exe
```

`--path <directory>`打开真实目录；`--fixture 1000000 --flow 8`使用明确标记的生成元数据渲染场景。测试包需完整解压ZIP，保留lib和许可证目录。

完整检查已包含无头门禁；只运行交互回归可用`./scripts/native/test-headless.ps1`。它覆盖51项GPUI事件、焦点、错误提示、菜单家族稳定性、真实文件/搜索结果、原位刷新与框选滚动，输出日志和JSON；PR及staging/main相关推送自动执行，测试包发布也必须通过。另有真实隐藏Win32菜单样式/父窗边界测试；系统原生集成与GPU验证范围见[回归分层](docs/native-resume-checklist.md)。

[preview.42 证据](docs/evidence/preview-42)、[preview.41 证据](docs/evidence/preview-41)、[preview.40 证据](docs/evidence/preview-40)、[preview.39 证据](docs/evidence/preview-39)、[preview.38 证据](docs/evidence/preview-38)、[preview.37 证据](docs/evidence/preview-37)、[preview.36 证据](docs/evidence/preview-36)、[preview.35 证据](docs/evidence/preview-35)、[preview.34 证据](docs/evidence/preview-34)、[preview.33 证据](docs/evidence/preview-33)、[preview.32 证据](docs/evidence/preview-32)、[preview.31 证据](docs/evidence/preview-31)、[preview.30 证据](docs/evidence/preview-30)、[preview.29 证据](docs/evidence/preview-29)、[preview.28 证据](docs/evidence/preview-28)、[preview.27 证据](docs/evidence/preview-27)、[本版证据](docs/evidence/root-case)、[搜索缓存证据](docs/evidence/search-cache-v43)、[第三方命令证据](docs/evidence/shell-commands)、[设计约束](docs/design-contract.md)、[研究](docs/research.md)、[交付记录](docs/delivery.md)。

文件区变更刷新保留现有画面、滚动与身份可验证的选择，新快照及可见页就绪后原位提交；失败保留旧画面。见[刷新约定](docs/grid-refresh-v40.md)。

存储与选择约定见[设计与实现说明](docs/storage-v41.md)；物理插拔与最终可见窗口回归仍按确切版本单独记录。

应用logo已改为圆叶小表情；[独立品牌稿](design/Backleaf-brand-v2.fig)和[生成/导出说明](docs/brand-v2.md)保留明暗与小尺寸检查，界面主稿为v43。

容量副文案10px，文案与细用量条一体展示；统一B/KB/MB/GB自动单位及整数舍入，见[容量展示](docs/capacity-v42.md)。

搜索结果按完整查询条件缓存在有界磁盘LRU中；普通路径按递归通知失效，索引搜索按变更ID更新，见[缓存与边界](docs/search-cache-v43.md)。

默认搜索的索引可用性判定、范围查询与静默回退见[搜索自动使用索引](docs/search-auto-index.md)。

索引维护的构建/重扫/增量成本、profile 与优化顺序见[索引维护](docs/index-maintenance.md)。
