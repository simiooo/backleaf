# 拾径 / Backleaf

Windows 原生 GPUI 文件管理器。当前测试版为 0.1.0-preview.14；项目范围跟随 Nuto SIM-919、SIM-934，交付后进入产品验收。

当前可编辑设计稿：[OpenPencil v38](design/Backleaf-v38.fig)，233页，保留此前所有基线与交互修订。版本索引见[设计目录](design/README.md)。设计先于实施；旧版design/Backleaf.fig保留为v6历史基线。

已实施原生虚拟文件网格、长文本、统一快捷访问与FileView、搜索和本地元数据索引、文件操作及进度、归档、目录变更监听、路径/查询恢复与独立窗口。本版加入原生OLE文件拖放和独立自绘菜单窗口，第三方扩展使用隔离宿主发现与执行。实测范围和限制见[测试版说明](docs/native-test-release.md)，不将未验证的桌面手势或所有扩展兼容性写成已通过。

Windows/MSVC/Rust 1.99.1：

```powershell
./scripts/native/bootstrap.ps1
./scripts/native/check.ps1
./target/release/backleaf.exe
```

`--path <directory>`打开真实目录；`--fixture 1000000 --flow 8`使用明确标记的生成元数据渲染场景。测试包需完整解压ZIP，保留lib和许可证目录。

[本版证据](docs/evidence/preview-14)、[上一版证据](docs/evidence/preview-13)、[设计约束](docs/design-contract.md)、[研究](docs/research.md)、[交付记录](docs/delivery.md)。
