# Backleaf 0.1.0 · 正式 Release

- [正式发布页](https://github.com/simiooo/backleaf/releases/tag/v0.1.0)，标签 `v0.1.0`，2026-11-08T14:38:51Z 发布，`draft=false`、`prerelease=false`，已设为 GitHub Latest。
- 版本 `0.1.0`，发布源码 `533cda6d5965c6cf8ef375c9a95f91a33bc0bb39`，已推送 staging。
- [正式 CI](https://github.com/simiooo/backleaf/actions/runs/42490064078) 首次运行的 x64、ARM64、publish 三个 job 全部成功。[CI 原始状态](ci-final.json)。
- [独立 headless CI](https://github.com/simiooo/backleaf/actions/runs/42490063618) 与 [installer CI](https://github.com/simiooo/backleaf/actions/runs/42490063588) 均成功。

## 正式发布流程

从 Cargo 版本重建 `0.1.0` EXE，提供 x64 / ARM64 包及正式 README。两架构并行完成 fmt、严格 Clippy、native、52 headless、优化构建、打包/CLI 与真实离线安装器 smoke 后，单个 publish job 核验两份来源、版本、包内摘要、clean-source headless 与 installer 证据。七项资产上传到草稿并核对服务端摘要后，一次性公开正式 Release 并设 Latest。

安装器优先选择正式标签 `vX.Y.Z`；没有正式版时回退预览版，显式 `-Tag` 仍支持预览。正式标签须绑定完整 40 位源码提交，包内构建来源须逐字一致。27 项回归在 Windows PowerShell 5.1 / PowerShell 7 均通过，覆盖正式版优先、预览标签、正式安装、分支目标拒绝和相同短前缀的不同完整提交拒绝。

`test-headless.ps1 -Output *.json` 现在将原始输出写到同名 `.log`，JSON 报告不再覆盖原始错误栈。本机以 JSON 路径执行 52 项回归通过，分别保留原始日志和报告。此修复针对诊断；preview.52 首轮 ARM64 属性窗口用例失败的根因未据此宣称修复。正式 CI 两架构均一次通过。

## 发布后实际核验

- 2026-11-08T14:40:20Z 完成七项发布资产下载：逐项匹配 GitHub 服务端 SHA-256，并核对两份 SHA256SUMS。[原始记录](verification.json)。
- 两个发布 EXE 的图标组 ID 1 均包含九档与 v3 导出逐字节相同的 PNG；[x64](icon-resources-x64.json)、[ARM64](icon-resources-arm64.json)。Logo、品牌稿、7z.dll、EXE、clean-source headless 报告匹配构建记录。
- x64 包于 2026-11-08T14:40:15Z 部署到本机独立 staging，app-local runtime 与实际 EXE CLI smoke 通过。ARM64 包于 14:40:20Z 解压静态核验；ARM64 代码执行及安装冒烟来自 hosted ARM runner。
- Windows PowerShell 5.1 指定 `v0.1.0` 的匿名真实安装成功，实际 EXE CLI / 运行库加载与摘要核验通过：[记录](installer-online-5.json)。
- PowerShell 7 省略 `-Tag` 的匿名真实安装选择了 `v0.1.0`，实际 EXE CLI / 运行库加载与摘要核验通过：[记录](installer-online-7.json)。两种安装均为独立 staging，不改用户默认安装目录或快捷方式。
- 发布器 `VerifyOnly` 正例及缺少 ARM64 安装证据、ZIP 字节改动、预览版冒充正式版三个反例通过：[预检](publisher-preflight.json)。

| 文件 | SHA-256 |
| --- | --- |
| x64 ZIP | `dbcf0fb69d80a29029d1a9b3688cb395f033734e53fca1f26bf8ae6488faf43b` |
| ARM64 ZIP | `636fb7b9a49a6bf8e68988d9213cd5b96e3d3df32f84d7d0038c576ad8cf3093` |
| x64 EXE | `9ab5483719a7f6f943c873896667734cf9eb73e33c87d0041cd880d43790e0a7` |
| ARM64 EXE | `edc61c39d6bb4d9dc17bba90eaf465c7c445de01adbba479dcc53a695003adc0` |

构建记录：[x64](build.json)、[ARM64](build-arm64.json)。安装器回归：[PS5.1](installer-tests-5.json)、[PS7](installer-tests-7.json)。本机 fmt/严格 Clippy/native141/headless52/Release build 均通过。

发布采用已交付 preview.52 的功能与 v3 图标，不扩大第三方扩展、屏幕阅读器、混合 DPI、物理 ARM 设备及长期压力的验证范围；已知范围见[版本说明](../../release-v0.1.0.md)。工程交付后进入产品验收。
