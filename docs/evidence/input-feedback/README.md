# v13 产品反馈修正证据

设计 master `design/Backleaf-v13.fig` 共 101 页，保留原 92 页并新增 9 页；新页冷重开像素比较通过，包含输入层级、箭头展开、常规文件名字重、失焦重命名和保留错误值的状态。

`raw/interactions-build.json` 标识 Windows 实机交互使用的构建。真实键盘检查包含 Ctrl+F、数字 `1`、Ctrl+A、字母 `b` 的中文预编辑、Space 提交「吧」；确认输入到达输入框，组合期间不把方向键作为结果导航。重命名 A 后直接右键 B，实际 Shell 成功提交名称，B 的上下文菜单打开；A 内容保持原字节，B 未修改。第二次重命名接续中文文件夹菜单，正确提供「粘贴到文件夹」。非法 `bad/name.txt` 失焦保留输入、显示错误、恢复焦点，不触发目标菜单。

`rename-blur-menu.png` / `.uia.txt` 保留菜单接续证据。箭头点击展开与再次点击收起通过；hover 自动展开及面包屑 tooltip 的处理器已从源代码移除，文件 tooltip 绑定于独立名称区域。常规字重用于所有选中态，维持名称度量。

`diagnostic-before-shell-fix.json` 对应最初的诊断构建。最初失焦实测暴露混合路径分隔符被 Shell 拒绝（E_INVALIDARG），原文件与输入均保留；修复后同一个原生行为测试覆盖规范和混合分隔符，校验实际文件内容、旧路径消失与成功账本。最终格式、Release Clippy `-D warnings`、30 个 Release 行为测试通过。两次后续视觉修正仅调整错误浮层横坐标及移除目录菜单 tooltip；最终构建与 staging 的映射另见 `raw/local-staging.json`，不混淆交互构建哈希与最终 CI 哈希。

preview.4 的 CI 41816485817 已完成构建/打包/解压 CLI 冒烟，但上传校验文件遇到 GitHub 502，发布撤回，未将其作为可用搜索版本交付。preview.5 因后续输入法 Enter/Esc 问题而取消 CI，未发布。preview.6 保留草稿发布、资产上传重试和服务端 digest 校验。preview.6 已于 2026-11-04 20:21:43 +08:00 发布；CI 与发布入口、最终版本、时间、校验记录见 `raw/release.json` 及 `raw/release-verification.json`；CI 执行的 CLI 冒烟与本机可见窗口分别列证。

本次不宣称整个 SIM-954 / SIM-920 完成。Explorer 文件剪贴板互操作、失败项重试、全局持久索引、保存搜索和恢复仍保留原验收事项；目录末行被选择计数覆盖的尾部滚动避让已登记，尚未包含本次修正。

对最终源码重新编译的 Shell 探针通过 12 种隔离案例，并单独通过 >260 UTF-16 单元路径复制及内容相等检查。原始结果摘要为 `feedback-shell*.json`；取消、缺失源、子项失败和源身份替换的结果按预期分类，未将其当全部成功。

同一最终 EXE 的深色启动也经过可见窗口检查：Ctrl+F 后输入框使用独立的 input-active 表面与阴影，真实数字键可写入。截图 `dark-active-input.png` 与 `dark-build.json` 标记该次检查。

preview.6 补充真实按键 x+Enter 的查询更新和组合期 Esc 保持搜索开启，见 `ime-enter*` / `ime-escape.uia.txt`。最终候选失焦检查使用 `ime-retain-build.json` 所示构建：未确认 b 被系统撤回时，保留 b.txt 原输入并显示保护提示，磁盘仍为 a.txt；确认后再次右键正常提交 b.txt 并打开目标文件夹菜单，见 `ime-safe-blur-menu*`。此边界仍有明确限制，未列为全输入法自动完成通过。

最终本机 staging 在 2026-11-04 20:14 +08:00 部署源码 ea34992b，EXE SHA256 bdb8dc8ed6d543ba5c0ed2d2e9b4f1dbf4cdf2e4fdff15c04a5c4ff729fbab89。CLI 与未锁屏状态通过；同一已部署 EXE 的真实 1 键、x 预编辑+Enter 得到 b.txt 一项、b 预编辑+Esc 保留搜索与焦点均通过，证据见 `local-staging-preview6.json` 与 `staging-preview6-*`。托管 CI 二进制单独记录，不将不同编译机 EXE 哈希混为同一。

[preview.6 测试包](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.6-ea34992b) 对应源码 ea34992b；[CI 41827367814](https://github.com/simiooo/backleaf/actions/runs/41827367814) 最终 success，30 项测试、Clippy、Release 构建、打包/解压 CLI 冒烟和草稿逐资产发布均通过。下载 ZIP SHA256 `eea02831a20d540c2270265d611b90cf86973769e945855cb293b0b244ca6d33`；与原 CI 产物及服务端 digest 相等，ZIP 内 EXE/设计/Logo 校验通过，未运行下载的 EXE。CI EXE 与本机可见验证的独立编译哈希分别记录。
