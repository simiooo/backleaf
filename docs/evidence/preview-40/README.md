# preview.40 · Shell 异步次级菜单空状态

源码：`7fdc9f3594bd9322f79ffa775c0e439da53dbee1`（commit `7fdc9f3`，2026-11-07）
候选版本：`0.1.0-preview.40`

- Release：[test-v0.1.0-preview.40-7fdc9f35](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.40-7fdc9f35)，pre-release，非 draft，target 为上述源码。
- Package CI：[42376426571](https://github.com/simiooo/backleaf/actions/runs/42376426571)，5m41s，全部步骤成功：fmt、严格 Clippy、129 项逻辑/文件/原生检查、49 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.40-7fdc9f35-win-x64.zip` | `a3d85b83fd18658740ca81eb7304d310be4463e497975be95dbb7a7c14de24d1` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `c3c532aad04545787fecdb503de93b5e7483b0d3971aefd461390d89f96af0ca` |
| `SHA256SUMS.txt` | `dcf27fee99316f2d69767aa27fb1bff76a719780401f917a24d6e53049a2502e` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `eb9c8c09108636ea9683c32354958937fbb438355e6142f4b15f1b98cf5a73a0`，headless 49 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批修复：异步次级菜单空白面板

触发链：

1. 右键菜单中的 Shell 扩展（如“授予访问权限”）是异步查询的次级菜单。
2. 点击后创建列，先等待 `ShellRows` 返回；期间不创建弹窗（避免加载占位窗口反复调整大小）。
3. 当 Shell 返回空列表或只有分隔线时，旧代码把该列设为 ready 且 `rows=[]`。
4. `column_geometry` 仍返回高度 32px（仅内外边距），弹出窗口因此是一个空白的窄面板——截图里的“半个元素”。

修复：

- 次级菜单 `ShellRows` 为空（或分隔线清理后为空）时，写入一行禁用的“没有可用的菜单项”，保持正常 `row_height` 与内边距，弹窗高度正常，不再出现空白半截面板。
- 查询失败仍显示“扩展暂不可用”。
- 未就绪的异步列继续不创建窗口；`late_dynamic_rows_do_not_revive_closed_columns_or_the_menu_family` 等既有行为不变。
- 新增 headless 回归 `empty_async_shell_submenu_keeps_a_readable_empty_row`：空结果后弹出窗口，断言 `column_geometry(2).height > 40`（旧空白面板为 32），随后 Esc 关闭整族。

验证：全部 49 项 headless、129 项 native、fmt、严格 Clippy、release 构建通过。

限制：真实 Windows Shell 扩展的返回内容因第三方软件而异；本修复只处理“空结果/无可用项”的展示，Shell 查询失败的真实原因仍只显示统一“扩展暂不可用”。未做真实 GPU 截图。

其它内容沿用 preview.39（容量单位/字段错误）、preview.38（焦点不重解码）、preview.37（预览溢出/通知合并）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
