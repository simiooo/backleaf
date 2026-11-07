# preview.41 · 内容搜索（grep 式文本搜索）

源码：`81c141f116c27a97554ca26c1f95ce73b7c464f1`（commit `81c141f`，2026-11-07）
候选版本：`0.1.0-preview.41`

- Release：[test-v0.1.0-preview.41-81c141f1](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.41-81c141f1)，pre-release，非 draft，target 为上述源码。
- Package CI：[42378155393](https://github.com/simiooo/backleaf/actions/runs/42378155393)，8m29s，全部步骤成功：fmt、严格 Clippy、131 项逻辑/文件/原生检查、50 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.41-81c141f1-win-x64.zip` | `abc93dfd9823b55eff741dac39816cc3f8e10b1070b2ab163c9cd5cc1e766e9a` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `18729fad86de897adc1b5cd35ccc026015b71b3931716afe4a4e7bad3af84d53` |
| `SHA256SUMS.txt` | `a32f57aba55ef40a4485b5020a9f33a4c83f379972db66f9736442e26545178f` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `10e8aedb56055103cbaf230c498484f9150f3669bce8b420be98bfb9db49b365`，headless 50 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批功能

### 内容搜索（`src/content_search.rs`）

- 搜索工具栏新增“内容”模式；文件名搜索仍为默认。内容模式从当前搜索位置**递归**检查文件内容。
- 文本判定按内容而不是扩展名：UTF-8（可带 BOM）、UTF-16 LE/BE（BOM）、无 NUL、控制字符比例 ≤ 0.5%；非法 UTF-8 视为非文本（GBK 等旧编码暂不支持）。
- 跳过符号链接/重解析目录循环（canonical 目录集合），忽略无法读取的目录/文件。
- 边界：单文件 8MB、20,000 文件、8,000 目录、累计 256MB、最多 200 个结果；达到上限标记“结果已截断”。
- 匹配为 ASCII 大小写不敏感；每个结果保存文件内首次匹配行号、上下文 3 行（前后各一行 + 匹配行，过长行按字符窗口截断）、所有结果内匹配的高亮字节范围、文件内匹配总数。
- 每次查询单后台线程扫描，新查询通过 latest token 取消旧扫描；发送分批结果（8 条/批或每 256 个文件进度）。

### 响应式卡片 UI

- 内容模式使用绝对定位的全内容区面板（从侧栏右侧到窗口右边，标题栏下方到底部），占据大部分应用空间。
- 卡片网格按可用宽度计算列数（最小卡片 260px），1–N 列响应式；`uniform_list` 只渲染可见行。
- 卡片无边框、无分割线：用间距、名称 13px semibold、路径/行号 10.5px secondary、匹配数、文件图标组织。
- 卡片主体就是匹配上下文块（“cover”），用 `StyledText` 对上下文内所有匹配文本加浅色背景高亮；行号与完整路径显示在内容块上方。
- 点击卡片或按 Enter 定位到匹配文件；空/加载状态有独立文案；内容模式隐藏筛选/保存视图按钮（它们只适用于文件名搜索）。

### 验证

- `content_search` 单元测试：文本嗅探拒绝二进制、接受 UTF-8/BOM/UTF-16；上下文 snippet 保留前后文且高亮范围精确。
- 无头 `content_search_finds_text_with_context_and_highlights`：真实 fixture 文本文件内容搜索，检查结果、上下文与高亮。
- 全部 50 项 headless、131 项 native、fmt、严格 Clippy、release 构建通过。

限制：

- 不支持正则、全词匹配、GBK/Shift-JIS 等旧/其他编码，也不读取 Office/PDF 等富文本内容。
- 每次查询全量扫描（无内容索引/增量），大目录受上限保护但首查仍可能慢；结果上限 200 会截断。
- 卡片网格暂未做方向键焦点导航（鼠标点击/Enter 可用），未做真实 GPU 截图。

其它内容沿用 preview.40（Shell 子菜单空状态）、preview.39（容量单位）、preview.38（焦点不重解码）、preview.37（预览溢出/通知合并）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
