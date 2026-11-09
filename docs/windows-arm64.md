# Windows ARM64 测试包

Backleaf 从 `0.1.0-preview.51` 起提供 Windows ARM64 测试包。

- 下载：GitHub Release 中 `Backleaf-<version>-<sha>-win-arm64.zip`，解压运行 `Backleaf.exe`。
- 安装：同一行安装器按**原生系统架构**选择资产：x64 Windows 安装 x64 包；ARM64 Windows 安装 ARM64 包，即使当前 PowerShell 是 x64 模拟或 32 位进程（检测 WOW64、机器级 `PROCESSOR_ARCHITECTURE`、`Win32_Processor`、`RuntimeInformation.OSArchitecture` 与进程架构等信号）：

  ```powershell
  irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex
  ```

- 包内包含 ARM64 `Backleaf.exe`、ARM64 `lib/7z.dll` 和 ARM64 app-local VC runtime；PE 架构均为 `0xaa64`。
- 与 x64 功能范围一致（归档、索引/搜索、右键集成等），但仅在 `windows-11-arm` hosted runner 上完成构建、native/headless 测试、CLI smoke 和离线安装器 smoke；**未在实体 ARM 设备上做长时间试用**。
- 目前不提供 x86（32-bit）包。
- 若更换应用语言，Explorer 右键菜单的静态文案需要重新运行安装器刷新；应用内部文案会立即切换。
