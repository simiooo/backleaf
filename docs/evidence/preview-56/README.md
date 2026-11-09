# preview.56 · 管理员界面整理

源码：`a65df12b63c1bc5fb7c94fac74c741776dc73ce6`，版本 `0.1.0-preview.56`。

- [发布页](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.56-a65df12b)（x64 + ARM64 共 7 项资产），2026-11-09T09:39:13Z 发布。
- [两架构 CI 42593740064](https://github.com/simiooo/backleaf/actions/runs/42593740064)：x64 成功；ARM64 首次 headless 出现一次 `file_properties_open_separately_keep_geometry_and_preserve_full_long_path` 环境性超时（55/56），重跑失败 job 后通过并发布 ARM64 资产。
- 变更：受保护位置不可用时错误直接显示在内容容器中央（块居中、内部左对齐、无卡片/阴影/边框/按钮，状态栏不再重复漂浮错误）；管理员窗口右上角改为顶栏盾牌图标 + tooltip，移除整条提示条。
- `C:\System Volume Information` 即使管理员令牌仍被拒绝：该目录 ACL 默认不授予 Administrators，Windows 资源管理器同样无法打开；界面显示真实错误并说明，不改变 ACL、不循环重试。管理员可打开的普通受保护目录（如其他用户目录在 Administrators 有权限时）不受影响。
- 本机 fmt、严格 Clippy、205 项 native/headless、release 构建通过；发布资产由 CI 打包并安装验证。真实 UAC/管理员窗口交互仍为人工验收项。
