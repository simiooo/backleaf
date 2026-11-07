# preview.42 · 中文 / English 界面

源码：`5a7f7da12bb75bec451e79a83864c5e724ea1cb5`（commit `5a7f7da`，2026-11-07）
候选版本：`0.1.0-preview.42`

- Release：[test-v0.1.0-preview.42-5a7f7da1](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.42-5a7f7da1)，pre-release，非 draft，target 为上述源码。
- Package CI：[42380311729](https://github.com/simiooo/backleaf/actions/runs/42380311729)，8m40s，全部步骤成功：fmt、严格 Clippy、133 项逻辑/文件/原生检查、51 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.42-5a7f7da1-win-x64.zip` | `634a2ca05bf0df0fffe1c287e6ce799700b022503bdd3c08b9a135571e7d8505` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `1ba7f2bb1d295d983dbd645e0c2620552c7be1f543773b6fa1ab2e2d22ed5ef0` |
| `SHA256SUMS.txt` | `391e8dc43ba05ad7558fb3d5bccb4b0ea66932dcc3aedc2d8cb537acd4aee1c2` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `9611a42a42f672a2c43e12282854ff395ce9673bf3b0b3776bd67347cb66dcba`，headless 51 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批功能：i18n

### 运行时语言

- 仅支持简体中文与 English。
- 解析顺序：
  1. `BACKLEAF_LANG=en` / `BACKLEAF_LANG=zh` 显式覆盖；
  2. 状态目录 `language.txt` 的持久选择；
  3. Windows UI 语言（`GetUserDefaultUILanguage` 主语言 0x04 判为中文，否则 English）。
- `Ctrl+Shift+L` 即时切换两种语言，并把选择写入状态目录；下次启动沿用。
- 测试/无头环境默认中文，保证既有断言稳定；`init_with_dir` 只在正式入口执行系统检测。

### 实现方式

- 源字符串保持中文作为 key，English 使用紧凑静态表；`tr()` 返回静态引用，`tr_owned()` 处理动态/含格式的结果，`trf!` 宏先翻译模板再替换 `{}` 占位符（例如 `Searching · checked {} items`）。
- 未翻译的条目回退中文而不是空白，便于逐步补齐。
- 已覆盖：主窗口标题栏/状态、右键菜单全集、搜索与内容搜索/筛选/字段错误、文件操作与任务面板、详情窗口主要字段、侧栏常用位置、拖放/归档/恢复提示、常见验证与导航错误。
- Shell 扩展菜单文字由 Windows/第三方软件自身语言提供，不受应用语言控制。

### 验证

- i18n 单元测试：词典查找、未知条目回退、占位符填充、切换后静态文本变化。
- headless `language_shortcut_switches_chinese_and_english`：快捷键双向切换并恢复中文。
- 全部 51 项 headless、133 项 native、fmt、严格 Clippy、release 构建通过。

限制：首次覆盖以高频界面为主，少量深层/第三方错误仍可能显示中文；没有 ICU 复数/性别规则，也没有按窗口独立语言；`language.txt` 写失败时仅本次生效。未做真实 GPU 语言切换截图。

其它内容沿用 preview.41（内容搜索）、preview.40（Shell 菜单空状态）、preview.39（容量单位）、preview.38（焦点不重解码）、preview.37（预览溢出）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
