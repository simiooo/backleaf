# preview.33 · 路径面包屑响应式展开

源码：`ae231d756c106ecd89707b8f51063821dc2414d1`（commit `ae231d7`，2026-11-07）
候选版本：`0.1.0-preview.33`

- Release：[test-v0.1.0-preview.33-ae231d75](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.33-ae231d75)，pre-release，非 draft，target 为上述源码。
- Package CI：[42359374085](https://github.com/simiooo/backleaf/actions/runs/42359374085)，8m38s，全部步骤成功：fmt、严格 Clippy、124 项逻辑/文件/原生检查、43 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.33-ae231d75-win-x64.zip` | `f68fcb055658d6da74b7435fa9c1cd0d47c2d7006ad3ece84bb8562e66c44405` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `37ec9a5f643a535d4d3cdc18cb3a0744b5f8d2123b63e8f4dbc62815d5839c31` |
| `SHA256SUMS.txt` | `805f31be052ca0dd7a73dd14fbac37e83769a4fdc6dd65efce8d902d310cd4f7` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `1e565df474e6a37573feed591f82298a7e1bb8dbb38f91ba447c3bd5820c369e`，headless 43 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":true,"error":null}`。未替换用户默认安装目录。

## 本批修复：面包屑按实际宽度展开

- `app_navigation::path_available` 移除了 600px 固定上限；改为从窗口宽度扣除后退/上一级、搜索框、操作入口、窗口按钮、拖拽区与标题栏间距，再按操作入口实际额外间距计算。
- `navigation::breadcrumbs` 改为按测量宽度贪心填充：宽度足够时显示全部祖先；放不下时才在中间插入一个省略号，根目录优先保留，当前目录始终保留并在必要时截断；优先保留离当前目录最近的祖先。
- 面包屑宽度估算与真实渲染对齐：祖先 crumb = 标签宽 + 8px、上限 120px；分隔符 12px；省略号 24px；右侧下拉箭头和控件内边距 32px；当前目录至少保留 48px 可读空间。
- 更新 `navigation::tests::crumbs_preserve_unc_roots_unicode_and_exact_navigation_targets`：窄/宽宽度可见段数单调不减，900px 下深路径不再出现省略号，UNC/Unicode 根仍保持正确目标。
- 示例：`D:\work\...\node_modules\@yuku-codegen` 在宽窗口下会继续展开更多祖先，而不是固定 `D:\ › … › node_modules › @yuku-codegen`。

限制：几何行为由单元测量与无头回归覆盖，具体像素排版仍需你的窗口实测确认；如果仍偏早/偏晚省略，可继续调 crumb 上限（120px）或当前目录最小保留宽度（48px）。

其它内容沿用 preview.32（框选滚动加速）、preview.31（空查询）与 preview.30（索引/搜索加速）。
