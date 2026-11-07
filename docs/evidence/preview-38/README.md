# preview.38 · 焦点/最小化不重解码图片

源码：`a36e0f186bbed1223eefd3210e8ea4aa9261dbe5`（commit `a36e0f1`，2026-11-07）
候选版本：`0.1.0-preview.38`

- Release：[test-v0.1.0-preview.38-a36e0f18](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.38-a36e0f18)，pre-release，非 draft，target 为上述源码。
- Package CI：[42371164813](https://github.com/simiooo/backleaf/actions/runs/42371164813)，6m53s，全部步骤成功：fmt、严格 Clippy、127 项逻辑/文件/原生检查、47 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.38-a36e0f18-win-x64.zip` | `432f590936facad4b4799b3dab1179ee300edd0f76f38231da239a06c224078d` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `02e132a198c3ccd2335683480614b907fb745da9e7f4875bd1a9b2a451137eea` |
| `SHA256SUMS.txt` | `057d5ed38013d4fca344769da71ffcba8388e8c25a9aaaa59b8b435a24fba835` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `cb75357d16628d99518c6b809bff176ffd2faff1bccb79d1dc1b68b1011b3874`，headless 47 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批修复：焦点/最小化不再触发图片重解码

触发链：

1. 窗口激活观察器每次获得焦点都调用 `mark_directory_dirty` + `refresh_search`，即使文件系统没有任何变化。
2. 450ms 后目录刷新提交，`generation` 递增；缩略图缓存原先以 `generation` 为 epoch，于是清空全部图片并重新解码/请求。
3. 最小化后重新聚焦、切走再切回都会重复上述过程。

修复：

- 激活观察器只在 `watch_stale()` 为真（watcher 实际报告内容变化）时标记目录刷新；无变化时不刷新。
- 缩略图缓存失效改用 `watch.applied`（已应用的 watcher 内容 epoch），不再使用每次记账刷新都递增的 `generation`；内容未变的目录刷新保留已解码预览，`tile()` 命中缓存直接返回 `ImageSource`，不排解码任务、不发通知。
- 仍只为当前渲染行请求缩略图；未完成的请求按 path 去重；解码通知仍按 20ms 合并（preview.37）。
- 新增 headless 回归 `image_preview_cache_survives_unchanged_directory_refresh`：刷新导致 generation 变化但内容 epoch 未变时，已解码图片仍在缓存中。

等价于用户提出的“记录渲染状态/hash”：缓存以绝对路径 + watcher 内容 epoch 记录“已解码且代表当前内容”，只有可见行会发起请求；没有引入逐像素 hash（路径/监听 epoch 已能判定内容是否变化，逐帧 hash 反而更贵）。文件真实变化时 epoch 变化，缓存失效并重新解码。

限制：焦点切换本身仍会重绘一次窗口（活动/非活动选中色、悬停清理、窗口状态变化），这是必要的状态变化；但不会再清空缓存或重新解码图片。若仍看到视觉闪烁，需要进一步区分是 GPUI 纹理重传还是其它元素的动画触发。

其它内容沿用 preview.37（溢出与通知合并）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
