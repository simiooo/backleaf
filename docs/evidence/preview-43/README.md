# preview.43 · 内容搜索浮层与查询优化

源码：`388aa93258931c0df07d85c1e4f37146056dbb44`（commit `388aa93`，2026-11-07）
候选版本：`0.1.0-preview.43`

- Release：[test-v0.1.0-preview.43-388aa932](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.43-388aa932)，pre-release，非 draft，target 为上述源码。
- Package CI：[42381943248](https://github.com/simiooo/backleaf/actions/runs/42381943248)，8m17s，全部步骤成功：fmt、严格 Clippy、134 项逻辑/文件/原生检查、51 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.43-388aa932-win-x64.zip` | `8eb5ffee828912b2777c918610bc356d3386a76835efd7ce9f7673789a6a307a` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `ee3680b5d8b5369aebe497508216597aee443273c948d98a9832e29b71839ce5` |
| `SHA256SUMS.txt` | `286b25a1723067b84dba9ec7060119730f5ee797ca1cd24e564e733be7b98250` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `acede61658af149de363ffcdfaa3a173c25fdddef2391c6dcc8968f350ea557a`，headless 51 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":true,"error":null}`。未替换用户默认安装目录。

## 本批改动

### 1. 浮层精确覆盖文件列表表面

- 内容结果面板不再使用“内容区左缘 + 16px”的近似尺寸，而是使用与文件表面完全一致的外接矩形：
  - `left = sidebar_width + SURFACE.margin_x`
  - `top = CHROME.bar_height + SURFACE.margin_y`（相对标题栏下方 overlay）
  - `width = viewport - sidebar - 2 × SURFACE.margin_x`
  - `height = viewport - bar_height - 2 × SURFACE.margin_y`
  - 圆角 `SURFACE.radius`、背景 `surface/files`、阴影 `file_surface_shadows()`。
- 只覆盖视觉层，未替换底层文件列表；滚动位置、选择、缓存和 watch 状态都保持原样，关闭内容模式后列表原状恢复。

### 2. 匹配高亮独立配色

- 新增语义色 `highlight/match-surface` 与 `highlight/match-content`（Light/Dark 两套暖琥珀色 + 专属文字色），不再借用浅蓝 `InputActive`。
- 匹配块与蓝色选中/悬停明确区分；颜色进入 `design/color-system.json` 的规则说明。

### 3. 内容查询优化

普通文件名搜索已有持久索引、分页与缓存；内容搜索本次补齐：

- **8KB 前缀嗅探**：先读前缀判断明显二进制；只有疑似文本才继续读完整文件，减少对图片/视频等二进制的全文件读取。
- **会话文本缓存**：按“路径 + 大小 + 修改时间”缓存解码后的文本，也缓存“已确认非文本”的负结果；重复查询同目录不再重复读盘。
- 缓存上限 32MB / 1024 项，按 LRU 淘汰；负结果不占文本字节。
- 保留既有上限（单文件 8MB、20,000 文件、8,000 目录、累计 256MB、最多 200 结果）、取消令牌和 250ms 防抖。
- 仍未实现**持久全文索引**：首次查询仍需读取文本文件，缓存只在当前进程内有效；这是下一步可做的方向（SQLite FTS / 内容索引 + 增量失效）。

### 验证

- `content_search` 单元测试新增文本缓存复用与长度变化失效断言。
- 全部 51 项 headless、134 项 native、fmt、严格 Clippy、release 构建通过。
- 未做真实 GPU 截图；浮层尺寸按布局常量计算，需要实际窗口确认视觉贴合。

其它内容沿用 preview.42（中英界面）、preview.41（内容搜索）、preview.40（Shell 菜单空状态）、preview.39（容量单位）、preview.38（焦点不重解码）、preview.37（预览溢出）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
