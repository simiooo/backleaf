# preview.31 · 空查询不再保留搜索

源码：`35d0c8d23991a9dea57c8711e941930d2c5dfe75`（commit `35d0c8d`，2026-11-07）
候选版本：`0.1.0-preview.31`

- Release：[test-v0.1.0-preview.31-35d0c8d2](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.31-35d0c8d2)，pre-release，非 draft，target 为上述源码。
- Package CI：[42356763958](https://github.com/simiooo/backleaf/actions/runs/42356763958)，5m50s，全部步骤成功：fmt、严格 Clippy、124 项逻辑/文件/原生检查、43 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.31-35d0c8d2-win-x64.zip` | `de949fa92f8a55ed6aedc4fefc57316bf7e43a5357782cdc641e9ab42d8050b4` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `f29f8d221eacb8137502ce058293f08cf16953fdfd924cbbf432f23f2cddf536` |
| `SHA256SUMS.txt` | `f0716a1e75897aa0fe1d0b08b7a29958e7e5af7cd926e29f340df2be38fc989e` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `c6c422046bf2da35e84b9065863adfa1ef2b13ad60df08bedd56423dfe9f07ce`，headless 43 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录（实际安装由你试用）。

## 本批修复

- Ctrl+F 打开搜索时，空输入只保留标题栏输入框，不再显示结果/工具栏弹层。
- 输入过查询后清空会直接退出搜索并恢复文件区焦点；点击弹层外区域也可关闭空搜索。
- 新增无头回归 `empty_search_query_does_not_keep_search_surface_open`，headless 从 42 提升到 43 项。
- 其余内容沿用 preview.30：持久索引维护（批量枚举、keyed diff、批事务/改名配对）、默认搜索自动使用索引、按父目录批量核验、2-gram 短查询、空闲调度、按需自动根与 LRU。
- 详细发布说明见仓库 `docs/native-test-release.md`。
