拾径 / Backleaf Windows x64 便携测试版。解压 ZIP，运行 `Backleaf.exe`。

本次加入左侧常用目录导航；顶部面包屑参考 Ant Design 与 shadcn，支持祖先链接、折叠菜单、当前位置下拉、hover 浮层和双击当前位置编辑，保留 Ctrl+L / Alt+D。文件及空白处右键打开上下文菜单，详情由菜单或 Alt+Enter 显式打开。选中计数改为内容宽度的悬浮盒，不再占用底部一整行。新 Logo 继续嵌入 EXE 与窗口。

文件操作处于验证阶段，本次适合在测试目录中试用；保存搜索分组、全局索引与启动恢复仍在研发。本地已检查导航、路径输入、菜单与新建流程，CI 执行格式、Clippy、行为测试、Release 构建及解压后 EXE 的 CLI 冒烟；不把托管 CI 当作可见桌面验收。

可编辑 OpenPencil v11 共 84 页，随 ZIP 提供，也可单独下载。具体源码与 CI 入口见 `build.json`，文件校验值见 `SHA256SUMS.txt`。
