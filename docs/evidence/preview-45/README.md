# preview.45 · 侧栏用户目录名称

源码：`2fbbaa727fe8806ddfae0ccda4f62e3cf0a6dc52`（commit `2fbbaa7`，2026-11-07）
候选版本：`0.1.0-preview.45`

- Release：[test-v0.1.0-preview.45-2fbbaa72](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.45-2fbbaa72)，pre-release，非 draft，target 为上述源码。
- Package CI：[42386731921](https://github.com/simiooo/backleaf/actions/runs/42386731921)，9m51s，全部步骤成功：fmt、严格 Clippy、135 项逻辑/文件/原生检查、51 项 GPUI 无头交互、installer 回归、打包与 CLI smoke、发布；发布构建继续应用 `vendor_patch=directwrite-font-update-no-check` 并重新构建。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.45-2fbbaa72-win-x64.zip` | `6292877e1d56dfc9e80a24afec6b258e1b30ef24123d8364cfded2a4096c5bed` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `c315c317bcf3f4fc9b057178bcbe9c40d7ce57765949d44875a0b0126456dbfa` |
| `SHA256SUMS.txt` | `2ae968994806d0cbd30406cba27060cf12100023cc5c44704058536639f6f5f3` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `66accb37e7cdcadfc5c868f45a9d68dad2496cafa34fb979db07b75400174957`，headless 51 项，`vendor_patch=directwrite-font-update-no-check`，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批功能：侧栏家目录名称

- 左侧原“个人文件夹”入口现在取 `FOLDERID_Profile` 路径的最后一级文件夹名，例如 `C:\Users\qq651` 显示 `qq651`。
- 路径、图标、点击目标、索引根与排序均不变；仅显示标题变化。
- 中英文界面都直接使用系统文件夹名，不翻译；路径没有可用末级名称时回退为 `Home / 个人文件夹`。
- 新增纯函数 `profile_folder_name` 与单元测试 `profile_title_uses_the_home_folder_name`（普通路径、尾随斜杠、根路径、Unicode）。native 135 项，headless 51 项。

## 附带 CI 稳定性修复

前两次 preview.45 CI 在 hosted runner 上因 rename/context-menu 异步操作超过 10s 实时等待而失败（诊断显示编辑器仍持有 in-place rename 文本，不是产品逻辑断言失败）。headless `wait` 保持 10ms 虚拟时钟步进，把真实 deadline 从 10s 提到 20s；第三次 CI 9m51s 全绿。

## 验证

- 本地 fmt、严格 Clippy、135 项 native、51 项 headless、release 构建通过。
- 冷启动优化沿用 preview.44：首次可交互渲染中位数约 237ms（原 ~403ms），详见 [preview.44 证据](../preview-44/README.md)。

其它内容沿用 preview.44（冷启动）、preview.43（内容搜索浮层/查询优化）、preview.42（中英界面）、preview.41（内容搜索）、preview.40（Shell 菜单空状态）、preview.39（容量单位）、preview.38（焦点不重解码）、preview.37（预览溢出）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
