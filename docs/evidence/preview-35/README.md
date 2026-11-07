# preview.35 · 无任务时不显示任务入口与结果面板

源码：`0f676fb3d140737445881b2b5d2059e2b008f694`（commit `0f676fb`，2026-11-07）
候选版本：`0.1.0-preview.35`

- Release：[test-v0.1.0-preview.35-0f676fb3](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.35-0f676fb3)，pre-release，非 draft，target 为上述源码。
- Package CI：[42364180959](https://github.com/simiooo/backleaf/actions/runs/42364180959)，6m28s，全部步骤成功：fmt、严格 Clippy、125 项逻辑/文件/原生检查、45 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.35-0f676fb3-win-x64.zip` | `0e9830755dcefaa5667fdb1fb57606fba985dbd56c04c7bc15ca94eceb1a338b` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `45a2c19bad0063451808bf500896570dd77d01cd531281967652831e25076a6b` |
| `SHA256SUMS.txt` | `1bc84816275c682c0bd23fb60e4f4c344d51f9394224b16bbf31396508d3b24d` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `56aaa0ce99bfde40968cbbdfdcfa6382483990b18722c3a0af6de9792c4af1dd`，headless 45 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批修复：完成且不可重试的操作不再常驻

- 标题栏任务入口（数字 + 下拉）和右下角结果面板现在只为以下情况显示：
  1. 有进行中的任务（排队、准备、运行、取消中、收尾）；
  2. 有可安全重试的失败结果（复制/移动/回收站且 `retryable > 0`、尚未重试）。
- 文件操作完成且没有可重试失败项时，`visible_operations()` 不再返回该结果，`operations_changed` 会关闭结果面板并清空选中；顶部入口随之为空隐藏。
- 可重试失败仍保留结果面板与“重试”按钮；重试或关闭后入口/面板同步消失，保留既有失败重试能力。
- 新增无头回归 `finished_non_retryable_operations_hide_badge_and_panel`：构造 Queued → 非可重试 Failed，断言入口/面板隐藏；再构造可重试 Failed，断言仍可见。headless 现为 45 项。

边界：可重试失败在用户处理（重试或关闭）前仍属于“待处理任务”，因此继续显示入口和面板；如果你希望这类结果也自动隐藏，需要另加可恢复的失败中心，否则会丢掉重试入口。

其它内容沿用 preview.34（高级搜索空查询）、preview.33（面包屑响应式）、preview.32（框选滚动）、preview.31（空查询退出搜索）与 preview.30（索引/搜索加速）。
