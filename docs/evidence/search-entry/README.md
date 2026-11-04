# 搜索入口测试证据

源码基线包含 native navigation / menu checkpoint `95b9d0a`，本次增加 v12 搜索入口与只读递归名称搜索。设计源文件为 `design/Backleaf-v12.fig`，92 页，新增 8 页均经过冷重开像素比较。

`raw/local-build.json` 标记实际启动的本机构建 EXE SHA256、独立测试目录与时间。`native-search.png` / `.uia.txt` 是该构建的真实 GPUI 窗口，搜索中文「报」并找到子目录文件；`native-empty.uia.txt` 覆盖新查询清除旧结果与无匹配状态。实际输入焦点、Ctrl+F、自动检索、方向键选取、点击结果定位选中及路径隐藏内部前缀均已检查。新建文件夹、自动进入名称编辑、输入中文名称并 Enter 提交，也在自建夹具内通过。

`tests.txt` 为 30 项 Release 行为测试；`clippy.txt` 为 Release Clippy `-D warnings`。`search-100k*.json` 是同一个搜索 worker 的本机 CLI 探针，分别覆盖中文短查询与十万结果首尾分页；`search-reparse.json` 验证独立 junction 夹具不递归到目标。扫描耗时是单次本机样本，不代表 GPU 帧率、通用硬件阈值或持久索引性能。

本地可见窗口使用本机构建，CI 包执行解压后的 CLI 冒烟；两个二进制哈希分别记录，不声称远端 CI EXE 已在可见桌面运行。发布后的 CI、版本、校验与入口将补充在 `release.json`。

搜索保存视图、全局持久索引和启动恢复尚未完成；文件命令仍为实验性功能，SIM-954 的 Explorer 剪贴板互操作与失败重试验收仍保留。此次不是整个 SIM-919 / SIM-920 的完成声明。
