# preview.39 · 高级搜索容量单位与字段级错误

源码：`9b47c86b8240a35e3a1aac0a30d660f2b2fe6486`（commit `9b47c86`，2026-11-07）
候选版本：`0.1.0-preview.39`

- Release：[test-v0.1.0-preview.39-9b47c86b](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.39-9b47c86b)，pre-release，非 draft，target 为上述源码。
- Package CI：[42375067246](https://github.com/simiooo/backleaf/actions/runs/42375067246)，8m30s，全部步骤成功：fmt、严格 Clippy、129 项逻辑/文件/原生检查、48 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.39-9b47c86b-win-x64.zip` | `e7c2526ac3eb209e75d2234700217f4f1784308fd8ee5f315fd40cc4136d108d` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `2e4fd15df5a2330c609f193f9647db0e460378ae9ca5cea5a748606f56c86191` |
| `SHA256SUMS.txt` | `6304fec264c57e9af7a2c06919c167541bcb883bb4e19f117f93f1120f6f0548` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `619919fdd706363e1ae3b8fc820f1e95f41c6d6ce96220b97cc588a09b34eec2`，headless 48 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批功能：容量输入与表单规则

### 容量解析（`src/capacity_input.rs`）

- 二进制单位：B/字节、KB/KiB/千字节、MB/MiB/兆/兆字节、GB/GiB/吉/吉字节、TB/TiB/太/太字节；也接受裸 `K`/`M`/`G`/`T`。
- 宽容格式：`10MB`、`10 mb`、`500 KB`、`1.5GB`、`2g`、`1024`、`3 字节`；数字与单位间空格可有可无，大小写不敏感。
- 拒绝：空值、无数字、未知单位、负数、溢出 u64；错误文案给出可用单位或示例。
- 普通数字按字节解释；小数按四舍五入到最近字节。

### 表单规则 review

| 字段 | 规则 |
| --- | --- |
| 类型 | 逗号/中文逗号/分号/空格分隔；可带开头 `.`；统一小写；最多 64 项；只允许字母数字，非法字符报错 |
| 最近天数 | 空=不限；支持 `7`、`7天`、`7日`、`7 天`；必须 1–365000 整数 |
| 最小/最大容量 | 空=不限；按上述容量解析；最小不能大于最大（错误标在最大容量） |
| 其他 | 空字段清除对应筛选；未通过校验不启动/重启搜索，保留上次有效条件 |

### 字段级错误 UI

- `FilterInput` 保存字段错误；`TextInput` 新增 `invalid` 状态，错误时边框使用 `Negative` 色。
- 错误文字显示在对应输入框正下方（红色、11px），并自动聚焦第一个错误字段。
- 用户编辑任意筛选字段时清除红框/错误文字；下一次失焦或 Enter 重新校验。
- 预填容量使用友好单位（如 `10 MB`），四个字段均增加占位提示（如 `如 10MB、500KB、1024`）。

### 验证

- `capacity_input` 单元测试覆盖 12 种合法格式与 7 种非法格式。
- 无头 `advanced_filter_capacity_errors_are_field_scoped_and_units_parse`：非法 `10XB` 产生包含“KB”的字段错误；`10MB`+`1MB` 在最大容量字段报“不能小于最小容量”；改为 `20MB` 后通过并写入正确的字节值。
- 全部 48 项 headless、129 项 native、fmt、严格 Clippy、release 构建通过。

限制：暂不支持千分位分隔（`1,024`）或数字+中文“万/亿”等非二进制单位；未做真实 GPU 视觉截图。筛选面板仍要求先有非空文本查询（preview.34）。

其它内容沿用 preview.38（焦点不重解码）、preview.37（预览溢出/通知合并）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
