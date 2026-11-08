# preview.47 · 标题栏三点菜单与“设为默认”

源码：`b49a0b1387026a623674df16174b8b6a0f74572d`（commit `b49a0b1`，2026-11-08）
候选版本：`0.1.0-preview.47`

- Release：[test-v0.1.0-preview.47-b49a0b13](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.47-b49a0b13)，pre-release，非 draft。
- Package CI：[42447874299](https://github.com/simiooo/backleaf/actions/runs/42447874299)，9m48s，fmt、严格 Clippy、136 项 native、52 项 headless、installer 回归、打包 smoke、发布全部成功。
- 资产 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.47-b49a0b13-win-x64.zip` | `764e39e9e7969cd0b10a38ee6f3002e9e1a0d88ef6a143557ba54a875951807c` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `829b3f4723aa8df1dbeea45af00e4704ec51fad8a812d7ea353bde641edf0867` |
| `SHA256SUMS.txt` | `81c0a2b746487893f711417355aa7041be2941c211da9bb0fc4d7661b9eed4ed` |

- 打包记录（[build.json](build.json)）：EXE `bfcd0856…`，headless 52 项，CLI smoke 通过，`vendor_patch=font-update-no-check+gpu-init-async`。

## 本批功能

### 三点按钮

- 位于右上角最小化按钮左侧，不占用 Windows 原生 caption 区域；窗口拖动、最小化、最大化/还原、关闭行为不变。
- 图标是独立的三点 SVG（新增 `overflow`），不是复用折叠 chevron。
- 窗口变窄时菜单宽度跟随客户区收缩，不越界；路径可用宽度同步扣除新按钮，面包屑不会因此变早省略。

### “设为默认”与运行形态适配

菜单首项目前按运行位置适配：

| 运行形态 | 菜单表现 | 点击行为 |
| --- | --- | --- |
| 安装版（`...\Programs\Backleaf\current\Backleaf.exe`） | “设为默认”可点，右侧标注 Windows | 打开 Windows“默认应用”设置页，由用户在系统内确认 |
| 便携版（ZIP 解压目录） | 禁用，显示“便携版不写系统默认项” | 不执行 |
| 开发版（`target\release`） | 禁用，显示“开发版不写系统默认项” | 不执行 |

Windows 不允许第三方应用静默把自己改成默认关联，所以本版**不做注册表写入**，只打开系统设置页并要求用户确认。这是明确的产品边界，后续若要真正替换资源管理器需要另做可恢复的关联注册与验收。

### 行为细节

- 跟随浅色/深色主题：floating surface、边框、阴影、悬停和禁用色均取自语义色。
- Escape、点击菜单外内容、窗口失焦/隐藏都会关闭。
- 三点按钮自身支持键盘聚焦与 Enter/Space 展开；菜单项可键盘激活。
- 菜单当前只有“设为默认”一项，后续项继续加。

## 验证

- 新 headless 回归 `titlebar_overflow_button_toggles_the_app_menu`：三点按钮打开/再点关闭。
- 新单元测试 `default_app::tests::installation_kind_is_derived_from_the_executable_path`：安装版/便携版/开发版分类。
- 真机验证：把发布包完整解压到模拟安装目录 `...\Programs\Backleaf\current`，启动后三点菜单展开，UIA 读到“设为默认”菜单项（338×40）为 enabled；点击后系统 `SystemSettings` 进程启动，菜单关闭。
- 截图 [menu.png](menu.png)：真实发布包、模拟安装路径、中文 Light 主题。截图顶部“更多”是鼠标悬停提示。
- 全部 136 项 native、52 项 headless、fmt、严格 Clippy、release 构建通过。

限制：本版没有真正成为默认程序/资源管理器；只提供系统设置入口。便携版/开发版故意禁用。若“之前说的默认项适配”指的是另一套规则（例如按 Windows 版本自动注册或指定具体默认行为），需要再按那条规则调整。

其它内容沿用 preview.46（GPU 并发实验）、preview.45（侧栏家目录名）、preview.44（字体缓存/启动追踪）、preview.43（内容搜索浮层/查询优化）、preview.42（中英界面）及更早 preview。
