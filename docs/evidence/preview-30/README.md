# preview.30 · 索引维护与搜索加速

源码：`e2f6b84a8929745f0eddae99af23cec8cb586166`（commit `e2f6b84`，2026-11-07）
候选版本：`0.1.0-preview.30`

- Release：[test-v0.1.0-preview.30-e2f6b84a](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.30-e2f6b84a)，2026-11-07 16:07:31 +08:00（08:07:31Z）；pre-release，非 draft，target 为上述源码。
- Package CI：[42354140117](https://github.com/simiooo/backleaf/actions/runs/42354140117)，6m32s，全部步骤成功：fmt、严格 Clippy、124 项逻辑/文件/原生检查、42 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.30-e2f6b84a-win-x64.zip` | `34a564bbaaf5ab143970cde16677a9ade24d9c5e755ee4517c75e5e60056de30` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `aa340e069420607106125e399b7140d8b388ea29f44b45e876e08836cf690a7e` |
| `SHA256SUMS.txt` | `28cd17f763adfabb3e24850c64ac7eea2e2ae426a4a4e64967d786aa51494288` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `dab3c70d73754d0e93d59c8c66fa9f601ad4f82a8d225030d60c2070889ab536`，设计资产 `216c6ffd…`，7z 库 `78bf6b66…`，runtime-v2，headless 42 项，CLI smoke 通过；MSVC 14.51.36231。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话文件输出 `{"locked":true,"error":null}`。未替换用户默认安装目录（实际安装由你试用）。

## 本批内容

- 索引维护：批量目录枚举、keyed 目录 diff、通知批事务/改名配对；2 万文件树构建约 17s、无变化启动重扫约 1.2s；10 万文件树构建约 93s、无变化重扫约 3.2s、索引约 205MB。
- 默认搜索自动使用已覆盖的持久索引，覆盖不足回退实时扫描；候选/可见页按父目录批量核验，2 字符查询使用 2-gram 倒排。
- 空闲调度与低优先级：系统空闲 15s 以上全速，有用户输入时限速；增量通知始终即时。
- 按需自动根 + LRU：机会根最多 8、总根最多 64；保存视图根固定不淘汰；驱动器根、临时目录和系统目录不自动索引。
- 顶栏点击收起路径下拉与搜索弹层，保留搜索框/路径按钮自身点击行为。

## 边界

- 基准为单机 release 样本；首次构建与索引体积受文件规模影响。
- 活跃限速路径未做人工输入基准；自动根/pin 的 App 接线由单元与无头回归覆盖，未单独做 UI 场景测试。
- 未引入 USN/MFT；索引仍为普通权限、单写者、身份核验与启动全量校准。
- 详细发布说明见仓库 `docs/native-test-release.md`，维护/搜索证据见 `docs/evidence/index-maintenance/` 与 `docs/evidence/search-auto-index/`。
