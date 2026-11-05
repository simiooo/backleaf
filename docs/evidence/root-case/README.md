# preview23：索引根大小写恢复与统一验收

源码2573f9127ab45d73d78658d0a7f798794ecc10ae，0.1.0-preview.23。延续OpenPencil v43/256页的范围变化与重建状态，没有新增界面。缓存实现与可编辑设计见[缓存约定](../../search-cache-v43.md)，此前实际第三方命令见[Shell证据](../shell-commands/README.md)。

在preview22实现上添加复现后，根目录仅改大小写再登记，使4个匹配文件得到7项；before-failure.txt保留失败。修复在索引写入事务内迁移实际根路径、清理旧父键并重建，合并旧数据库重复根，并依据根ID与路径重建监听；迟到通知不能复活已删除根。focused-after.txt为修复中的三项针对性测试；最终干净提交的tests.txt和headless.txt包含全部相同场景。

fmt、严格release/all-target Clippy、101项逻辑/真实文件/COM/隐藏Win32测试及31项生产GPUI无头回归通过。独立[无头CI](https://github.com/simiooo/backleaf/actions/runs/42037571335)也通过，下载的日志与JSON保留干净源码、31个用例、必需用例及日志摘要。无头回归验证真实旧数据库恢复、非递归结果无重复、运行中的根迁移和之后继续新增；不把它当作GPU/IME/OLE验证。

benchmark.txt在同一源码上显式运行：10,004个真实自有文件、10,003个匹配、5轮，使用持久化SQLite索引、生产搜索worker与首32项读取。文件创建与生产索引reconcile建库不计入查询时间；OS磁盘缓存未清空、普通桌面后台负载未控制。全量结果缓存未命中中位4856.1574ms、命中25.5652ms、单文件增量188.6495ms。命中0项候选重新匹配，单文件增量1项候选；每页最多32次身份验证。原始数组和机器/磁盘环境单列保存。这些数值不表示界面帧率或输入到像素延迟。

本地staging于2026-11-05 21:41:00+08实际部署并验证包内EXE、三个相邻运行库与启动诊断，EXE SHA256为4a62c79fa0e4c10247692d204b0230f5db5fcfe88239e171fdef75a7f1376d38。local-staging.json的interactive_smoke_passed=false表示该打包脚本不做窗口操作，不能将其当作可见检查记录。公开CI版本的安装与窗口证据另列，不能混用本机与CI的EXE摘要。

[公开测试包](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.23-2573f912)于2026-11-05 21:45:58+08发布，[发包CI](https://github.com/simiooo/backleaf/actions/runs/42037572677)最终success。101项非无头、31项无头以及Windows PowerShell 5.1 / PowerShell 7各20项安装回归通过。四项公开资产均匿名重新下载，实际字节同时匹配GitHub摘要和下载的CI artifact；ZIP SHA256为fcdacf6abb227f1ab61aea971ebb3161d3390a996acba1bc1d317ad5910c0368。ZIP内EXE、7-Zip引擎、可编辑设计和31项headless报告逐项匹配元数据。CI报告保留runner的原始行尾，因此不拿本地安装脚本的字节摘要替代CI脚本摘要。

2026-11-05 21:46:14+08从公开发布匿名安装至ci-install.json记录的隔离目录，相邻运行库加载和EXE诊断通过。CI EXE SHA256为032e59b6966609a60f366ded17137726f2b931cdd22ef92d0f5c59b24a18f6b1。此前用户Escape中断的阶段不计通过；本次用户明确恢复后，使用此确切安装版于22:08–22:10完成可见启动、右键与多层自绘独立菜单、动态CRC SHA、实际SHA-256校验文件生成、外部文件自动刷新和Alt+F4正常退出。进程26972与全部相关窗口均已退出，起止WTS均unlocked。首次退出后的即时枚举尚有旧窗口，随后的独立枚举为空且进程已不存在，两份原始记录均保留。

菜单中的第三方处理器是已安装的7-Zip 26.05，7-zip.dll SHA256为37bae1024808e0809116f2b8e7471d53d66cb8e8d3fe5fa3e4b34ffff2762851。实际点击“应用操作 → 7-Zip → CRC SHA → SHA-256 -> report α.txt.sha256”；输出包含源文件真实摘要2d9999e290799b63b065ccabbe0ddc580b52d8d8d73f19871e22ef9ce15122c5和完整Unicode名称，源文件未改变。校验文件字节摘要为edd47014b066d59247f68ac9c9636397c3d9d772d74544e55565e6b3888a5931。ZIP添加与校验信息窗口的具体命令证据仍对应preview22，不能改记为本版重复验证。

native-observations.json保留Computer Use实际返回的可访问文本和窗口几何，不包含截图载荷；截图在会话中直接观察。异步发现前后及逐级展开时，根菜单保持x893/y364/276×432，应用菜单保持x1228/y640/276×592，7-Zip菜单保持x1563/y760/276×256；动态CRC SHA为276×464。列按实际横向位置对应，瞬时z-order会因焦点而变化，不能作为持久窗口ID。这是所采样状态的几何验证，不宣称测得高速闪烁或GPU呈现延迟。

previous-hidden-process-cleanup.json记录仅清理先前被测试脚本隐藏启动的自有进程43036，核验EXE、命令行和创建时间后再启动可见隔离测试；没有终止用户应用。实际正常退出证据针对新的可见进程26972。真实USB插拔、完整NVDA/IME、混合DPI和Explorer全部手势仍遵循各自验证边界。
