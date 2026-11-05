# 拾径 / Backleaf

Windows 原生 GPUI 文件管理器。当前测试版为 0.1.0-preview.15；项目范围跟随 Nuto SIM-919、SIM-934，交付后进入产品验收。

当前可编辑设计稿：[OpenPencil v38](design/Backleaf-v38.fig)，233页，保留此前所有基线与交互修订。版本索引见[设计目录](design/README.md)。设计先于实施；旧版design/Backleaf.fig保留为v6历史基线。

已实施原生虚拟文件网格、长文本、统一快捷访问与FileView、搜索和本地元数据索引、文件操作及进度、归档、目录变更监听、路径/查询恢复与独立窗口。本版补齐生产视图的GPUI无头交互回归和CI门禁，并修复同名重命名的错误恢复；v38视觉基线不变。实测范围和限制见[测试版说明](docs/native-test-release.md)。

Windows PowerShell 5.1 / PowerShell 7，一行安装最新测试版，无需管理员权限：

```powershell
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex
```

从开始菜单打开 Backleaf。重复运行同一命令即可升级，升级前请关闭 Backleaf。安装目录为`%LOCALAPPDATA%\Programs\Backleaf\current`，用户数据仍保存在`%LOCALAPPDATA%\Backleaf`。脚本校验 GitHub 资产 SHA256、包内 EXE/归档库及版本来源，失败不替换已安装版本。[安装参数与验证](docs/install.md)。

Windows/MSVC/Rust 1.99.1：

```powershell
./scripts/native/bootstrap.ps1
./scripts/native/check.ps1
./target/release/backleaf.exe
```

`--path <directory>`打开真实目录；`--fixture 1000000 --flow 8`使用明确标记的生成元数据渲染场景。测试包需完整解压ZIP，保留lib和许可证目录。

完整检查已包含无头门禁；只运行交互回归可用`./scripts/native/test-headless.ps1`。它覆盖16项GPUI事件、焦点、错误提示、真实文件/搜索结果与框选滚动，输出日志和JSON；PR及staging/main相关推送自动执行，测试包发布也必须通过。系统原生集成与GPU验证范围见[回归分层](docs/native-resume-checklist.md)。

[本版证据](docs/evidence/preview-15)、[上一版证据](docs/evidence/preview-14)、[设计约束](docs/design-contract.md)、[研究](docs/research.md)、[交付记录](docs/delivery.md)。
