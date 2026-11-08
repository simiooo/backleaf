# preview.50 · Explorer 右键集成、隐藏菜单、内容搜索高亮与滚动条

源码：`4f403ddfb6f811521b9dfdb2190f75239c884232`
候选版本：`0.1.0-preview.50`

- Release：[preview.50](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.50-4f403ddf)
- Package CI：[42457387074](https://github.com/simiooo/backleaf/actions/runs/42457387074)，9m58s，全绿
- 测试：native 141、headless 52、installer 22；fmt、严格 Clippy、release 构建通过。
- 安装：`irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex`
- ZIP SHA-256：`c6d167b3e6c920da245c357f6e3d6d10709e44f0081253b2a08512bbc2f2494f`
- EXE SHA-256：`5e1a3d745f45612e841cc34d0b2e947a862e56e99a5aaf013ac0b90dc0f66cb8`
- 资产：`build.json` `a6098a8fb95d2707e1b1b3187ef7ddbed78ee127de80194b260e09795f74952e`；`SHA256SUMS.txt` `e4bdd5cfa570bfe6bec1b9eb394f1e5a1b9053bc154e5827a84d841cd27c9e57`

## 变更

1. **资源管理器右键集成（建议 1）**
   - 安装包在 HKCU Classes 注册三项（命令带 `--path`，避免把文件夹路径当成非法位置参数）：
     - `Directory\shell\Backleaf`：文件夹右键“在 Backleaf 中打开”，`"%1"`；
     - `Directory\Background\shell\Backleaf`：文件夹空白处右键，`"%V"`；
     - `Drive\shell\Backleaf`：驱动器右键，`"%1"`。
   - 命令指向安装后的 `current\Backleaf.exe`，升级时覆盖；不需要管理员权限；便携版不注册。
   - 验证：用 `Shell.Application` 枚举文件夹右键动词，返回“在 Backleaf 中打开”；修复 `--path` 后调用该动词成功启动 Backleaf 新窗口。注册表项测试后清理。截图见 [content-search-scrollbar.png](content-search-scrollbar.png) 仅展示应用内变化，Explorer 菜单由 Shell verb 验证。
2. **移除“设为默认”入口与三点菜单**
   - 删除 `app_menu`/`default_app` 模块、标题栏按钮、菜单 overlay、相关快捷键/失焦处理；
   - 删除对应 headless 用例，路径栏恢复原宽度；不再显示任何无效的“预设默认”控件。
3. **内容搜索高亮修复**
   - 之前卡片先显示匹配行的前一行；长 JSON/长行换行后会把匹配行挤出卡片可视区，出现“结果存在但看不到高亮”。
   - 现在 snippet 从匹配行开始，匹配行内窗口围绕关键词；高亮颜色保留暖琥珀并加粗；新增中文关键词、长行高亮单元测试。
   - 本地实机 300 文件搜索结果截图见 [content-search-scrollbar.png](content-search-scrollbar.png)：匹配行在卡片顶部并带暖色/加粗高亮。
4. **内容搜索结果滚动条**
   - 复用文件列表的 `Geometry`/滚动条交互和焦点/ARIA 语义：可见轨道、拖动滑块、点击轨道翻页、滚轮/PageUp/PageDown/Home/End；
   - 新增 headless 回归 `content_results_scrollbar_tracks_overflow_and_pages_the_list`，验证 200 条结果时出现滚动条且点击轨道后 offset > 0。
   - 截图见 [content-search-scrollbar.png](content-search-scrollbar.png)：右侧滑块可见，顶部不再有三点菜单。

## 验证

- `cargo fmt --check`、严格 Clippy、release 构建通过；
- native 141、headless 52、installer 22 回归通过；
- 安装包右键注册函数用本机 HKCU 临时项验证后清理；
- 发布包 SHA-256/EXE/安装 smoke 已通过；本机已安装 preview.50，HKCU 三项右键注册已生效，并通过 Shell.Application 动词启动新的 Backleaf 窗口。

## 限制

- Windows 仍然没有受支持的方式把第三方程序设为系统默认文件管理器；本次只做资源管理器右键集成，不接管 Win+E/任务栏/桌面。
- 右键项目前为中文固定文案；安装器只在默认安装目录注册，自定义 `-InstallDir` 不自动注册。
- 高亮修复针对数据/布局（匹配行置顶）与加粗；如果用户在特定 GPU/DPI 下仍看到背景缺失，需要再按环境复现。
