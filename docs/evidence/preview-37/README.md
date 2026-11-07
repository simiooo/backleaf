# preview.37 · 图片预览选中溢出与重绘修复

源码：`1b17b2207267eb0b71c20a6a985cec9bded6f57a`（commit `1b17b22`，2026-11-07）
候选版本：`0.1.0-preview.37`

- Release：[test-v0.1.0-preview.37-1b17b220](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.37-1b17b220)，pre-release，非 draft，target 为上述源码。
- Package CI：[42369512541](https://github.com/simiooo/backleaf/actions/runs/42369512541)，7m38s，全部步骤成功：fmt、严格 Clippy、127 项逻辑/文件/原生检查、46 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.37-1b17b220-win-x64.zip` | `fcc40d552b472be48dd9eb5846c524548942f3209e09b9565c423f382cf30848` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `aba9d2d3f32c78892b9c079391cfa50d90f1ee889708650ce0e33dc2e9926775` |
| `SHA256SUMS.txt` | `924937a0dd321a114b097390966c3c469d45ce0ba4e3130fa5a73c82bbe383ec` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `a0092a2b52cfb11628b1c3c55e82e22ada77607a13fcdf6e71e18ed11e929906`，headless 46 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批修复

### 图片溢出资器

旧底板只按“图标 44×44 或名称宽度”计算选中/悬停圆角面；图片预览容器是“单元格内宽（例如 96px）× 48px”，比底板宽，所以选中时图片会画出蓝色底板之外。

- 新增 `file_geometry::surface_with_art` / `caption_with_art`：底板宽高取 `max(icon, name, art)`，图片文件的底板现在覆盖整格宽度与高度（48px 高预览 + 36px 名称 + 内边距），图片水平/垂直都落在底板内。
- `file_background` 接收实际 art 尺寸；名称 tooltip/命中区域同步下移 4px，与更高预览对齐。
- 非图片文件的 art 尺寸仍是 44×44，旧布局与命中行为不变。

### 重复重渲染

- 旧实现每解码完一张图片就发送一次 `ImagePreviewsChanged`，每次都会触发整窗渲染；一次进入含几十张图片的目录会产生几十次重绘。
- 新增独立通知线程：解码结果每 20ms 合并为一次通知，UI 一次渲染可 drain 一批结果；线程在 Cache Drop 时停止。
- 已解码图片继续走 256 项 LRU 缓存，滚动/选中不会重复解码。

测试：`file_geometry` 增加 96×48 预览底板包络断言；无头缩略图测试与全部 46 项 headless、127 项 native 通过。

限制：未做真实 GPU 截图；重绘已按解码批次合并，但新图片真正到达时仍会有一次正常重绘。如仍觉得有异常高频重绘，需要确认触发源（例如悬停、滚动或某个动画）。

其它内容沿用 preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
