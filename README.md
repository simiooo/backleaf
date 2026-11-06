# 拾径 / Backleaf

Windows 原生 GPUI 文件管理器。当前候选版为 0.1.0-preview.27；项目范围跟随 Nuto SIM-919、SIM-934，交付后进入产品验收。

当前可编辑设计稿：[OpenPencil v46](design/Backleaf-v46.fig)，271页，保留此前所有基线与交互修订。版本索引见[设计目录](design/README.md)。设计先于实施；旧版design/Backleaf.fig保留为v6历史基线。

已实施原生虚拟文件网格、长文本、统一快捷访问与FileView、搜索和本地元数据索引、文件操作及进度、归档、目录变更监听、路径/查询恢复与独立窗口。本版在后台准备文件变化快照，原位更新并保留滚动及身份可验证的选择；搜索自动刷新也保留旧结果直到新页就绪。独立菜单与相邻运行库沿用preview16。本版补齐分区容量与使用率、PnP介质通知、非递归选中文件大小和重复字母循环定位。本版新增确认失败项重试（只重放已确认失败的顶层项，成功、未确认、取消及部分完成目录不重放），并完成 Explorer 文件剪贴板互操作实机验证。实测范围和限制见[测试版说明](docs/native-test-release.md)。

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

完整检查已包含无头门禁；只运行交互回归可用`./scripts/native/test-headless.ps1`。它覆盖40项GPUI事件、焦点、错误提示、菜单家族稳定性、真实文件/搜索结果、原位刷新与框选滚动，输出日志和JSON；PR及staging/main相关推送自动执行，测试包发布也必须通过。另有真实隐藏Win32菜单样式/父窗边界测试；系统原生集成与GPU验证范围见[回归分层](docs/native-resume-checklist.md)。

[preview.27 证据](docs/evidence/preview-27)、[本版证据](docs/evidence/root-case)、[搜索缓存证据](docs/evidence/search-cache-v43)、[第三方命令证据](docs/evidence/shell-commands)、[设计约束](docs/design-contract.md)、[研究](docs/research.md)、[交付记录](docs/delivery.md)。

文件区变更刷新保留现有画面、滚动与身份可验证的选择，新快照及可见页就绪后原位提交；失败保留旧画面。见[刷新约定](docs/grid-refresh-v40.md)。

存储与选择约定见[设计与实现说明](docs/storage-v41.md)；物理插拔与最终可见窗口回归仍按确切版本单独记录。

应用logo已改为圆叶小表情；[独立品牌稿](design/Backleaf-brand-v2.fig)和[生成/导出说明](docs/brand-v2.md)保留明暗与小尺寸检查，界面主稿为v43。

容量副文案10px，文案与细用量条一体展示；统一B/KB/MB/GB自动单位及整数舍入，见[容量展示](docs/capacity-v42.md)。

搜索结果按完整查询条件缓存在有界磁盘LRU中；普通路径按递归通知失效，索引搜索按变更ID更新，见[缓存与边界](docs/search-cache-v43.md)。
