# preview.34 · 高级搜索不再触发空查询扫描

源码：`a185f1b9960f59dd375745c42e840fbeea542be9`（commit `a185f1b`，2026-11-07）
候选版本：`0.1.0-preview.34`

- Release：[test-v0.1.0-preview.34-a185f1b9](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.34-a185f1b9)，pre-release，非 draft，target 为上述源码。
- Package CI：[42360799879](https://github.com/simiooo/backleaf/actions/runs/42360799879)，7m35s，全部步骤成功：fmt、严格 Clippy、125 项逻辑/文件/原生检查、44 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.34-a185f1b9-win-x64.zip` | `8c7f0db70c35d236b3eefd99fb86a6214bb3907af2aa06fc6ea5321ad1628695` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `77b77d92e210f2144091d00509032d2a14b81f1fe3b92d744bb401af85d5f55b` |
| `SHA256SUMS.txt` | `d576a823ef4a1da7aea754695e1409c103a6cc955c9dd51e67ca04b05b9df71f` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `53e725b7a524a8d27c7327f828595aa771a38451ea2809a9634d74631c752ac2`，headless 44 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批修复：高级搜索不触发空查询扫描

- 打开/编辑高级搜索（类型、最近天数、最小/最大字节）前要求有非空文本查询；输入法组合中或 250ms 防抖尚未提交的输入按“有查询”处理，不会误拦截筛选面板。
- 筛选或排序/范围/布局选项改变时，只重启已有文本的搜索；空查询不再调用 `start_search`。
- 搜索 Worker 增加空查询保护：即使从恢复的保存视图/其他入口收到空查询请求，也只发 0 项完成事件、不遍历磁盘。
- 关闭搜索时同时清理筛选编辑状态，避免下次打开残留面板触发旧逻辑。
- 新增测试：`search::tests::empty_query_worker_completes_without_scanning`、无头 `advanced_filters_require_nonempty_query`；headless 44 项、native 125 项。

边界：筛选条件目前需要配合文本查询使用（这是本批有意的行为）；如果后续希望“只按类型/大小筛选、不输入文本”也做全树搜索，需要单独的产品设计与预算控制，不能复用空查询扫描。

其它内容沿用 preview.33（面包屑响应式）、preview.32（框选滚动）、preview.31（空查询退出搜索）与 preview.30（索引/搜索加速）。
