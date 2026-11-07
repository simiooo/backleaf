# preview.32 · 框选边缘自动滚动加速

源码：`03d8f7333e906e438e1a5257be889f323a114390`（commit `03d8f73`，2026-11-07）
候选版本：`0.1.0-preview.32`

- Release：[test-v0.1.0-preview.32-03d8f733](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.32-03d8f733)，pre-release，非 draft，target 为上述源码。
- Package CI：[42357803392](https://github.com/simiooo/backleaf/actions/runs/42357803392)，6m19s，全部步骤成功：fmt、严格 Clippy、124 项逻辑/文件/原生检查、43 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.32-03d8f733-win-x64.zip` | `9bb2c84831314982e0e7b5fb74586ff8f84da215651f1ba24f03bd1c168f1105` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `9a163fee64b20f597d97a68297d5177b02d06181d77e2995ebc27e2d36edbef9` |
| `SHA256SUMS.txt` | `dd4ffe001ce68ae78916377c2593339e3bcb53f95673712e877b339fe95a945b` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `f5443f00f27b869bb0dbb284e1f1870aa7126e18e8340b0823efc32d76727396`，headless 43 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批内容：框选自动滚动标定

`design/marquee-scroll-system.json` 重新标定，目标是接近资源管理器框选的选择滚动，而不是之前的保守曲线。按 720px 视口折算：

| 状态 | 速度 |
| --- | ---: |
| 满深度（贴边）起步 | 约 1.8 屏/秒 ≈ 1300 px/s（旧值约 1.0 屏/秒） |
| 浅深度 0.2（靠近内容区） | 约 0.31 屏/秒 ≈ 225 px/s，保留精细控制 |
| 持续贴边约 0.7 s 后 | 约 5.3 屏/秒 ≈ 3816 px/s（旧值需约 3 s 才到同量级） |
| 大范围完整穿越目标 | 约 8 s（旧标定约 18 s） |

行为保持：返回内容区或反向立即回到精细档，松手无惯性，端点刹车/夹紧，长帧间隔不会追加大跳。`src/marquee_scroll.rs` 的 8 个控制器测试已随新标定更新，框选无头回归仍通过。

边界：这是按“产品对照目标”调的参数，**没有直接测量 Explorer 的私有滚动常量**；是否与你的 Explorer 体验一致需要实际并排试用确认。可在 `design/marquee-scroll-system.json` 调整 `fineScreensMaximum`、`fineRampGain`、`rangeTraversalSeconds` 等参数再发一版。

其它内容沿用 preview.31（空查询修复）与 preview.30（索引维护/搜索加速）。
