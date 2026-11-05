# preview.15 · SIM-992

本批次沿用233页OpenPencil v38视觉/交互基线，补齐生产Root、TextInput、MenuPopup、真实隔离目录/搜索/文件服务的16项GPUI无头交互回归，另保留81项逻辑/文件/COM协议测试（共97项）。用例和原生边界矩阵见[回归分层](../../native-resume-checklist.md)。无头只替换HWND/显示器边界，不通过桌面输入操作应用；字体为合成度量，未启用像素渲染或屏幕阅读器适配。

mutation-verification.json与mutation-*.log记录反向验证：临时禁用TextInput Blur订阅后，Tab切换焦点的保存用例按预期失败；在MenuPopup render重新读取owner实体后，菜单重命名入口按原始路由问题失败。两处源码均逐字节恢复。普通空白点击会走网格自己的提交路径，不能单独证明输入框Blur订阅存在，因此主失焦用例使用Tab，另测空白点击和切换右键。

回归发现并修复：已有同名目标时，内联重命名会进入Shell冲突等待。后台先核验目标并返回错误、保留草稿和焦点，不覆盖源/目标；另测仅改变大小写仍成功。错误提示检查保留的内容、焦点与实际绘制的浮层几何，不把TestPlatform未激活的无障碍树冒充为UIA证据。

GitHub新增PR/staging/main相关推送的只读无头门禁；发包流程也执行相同门禁，检查干净源码与日志摘要，包内携带headless.json。测试失败、未运行足量用例或报告不匹配时拒绝发布。最终本机/CI构建、native-only验证、发布与下载核对的版本和摘要会分别追加，不混用不同EXE的证据。

最终本机源码786c6e53acd66e7e0a139b30aa6b4dc75e464897，EXE SHA256为7c7564509afe5b305e220c25852cc1d018dfdbfa0459258068f0d4fdd3170527。81项原有测试和16项无头交互回归分别通过，干净源码无头复跑8.32秒；local-build.json记录编译输入和五类检查日志摘要，local-headless.json与ZIP内报告一致。2026-11-05T06:36:30Z解压部署至独立staging，版本和部署摘要见local-staging.json。

实机只检查本轮相关的原生边界：native-owned-popup.png是主窗口边界外完整的独立Win32浮窗，native-popup-geometry.json记录主窗与浮窗的实际屏幕位置和125%缩放；原生前台切换后菜单关闭、主窗口可用且正常退出。无头已覆盖的输入/搜索/重命名/框选流程未重复桌面操作。系统IME、UIA/NVDA、混合DPI/拔屏、Explorer完整跨应用拖放及长期压力仍不计为本轮已验证。

performance为同一本机EXE的真实可见GPUI控制器测量，1120×720逻辑尺寸、125%缩放：10万项CPU P95 5.0982ms、回调间隔P95 12.0731ms、79.07MiB；1000万项+8条流光CPU P95 5.541ms、回调间隔P95 12.8268ms、83.43MiB，框选6,999,826项身份正确并达到完整范围。两者缓存100项/129标签、单帧回调、隐藏动画请求为零。这是生成元数据场景，不是磁盘千万文件遍历、GPU present延迟或传输吞吐。

[无头CI 41981505538](https://github.com/simiooo/backleaf/actions/runs/41981505538)与[发包CI 41981507643](https://github.com/simiooo/backleaf/actions/runs/41981507643)均最终完成success。2026-11-05T06:50:20Z发布[preview.15](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.15-786c6e53)，ZIP SHA256为16b9083625104c8f05acffe14aaf9bf3fbc358fc6d017892c7ea75aa1e8bb397，CI EXE为acf3aef0547f608cb7113e31967255c4fd8bcac4c888092ad4dcd4b5db87b4bc。ci-run.json记录最终状态；ci-build.json、release.json及download-verification.json分别保留元数据、服务端摘要与下载对照。CI EXE与本机EXE不同，不能混用可见窗口证据。

用户按物理Escape停止了最后下载版窗口检查；未继续桌面自动化，该项不标通过。随后SIM-998在PowerShell 5.1/7匿名下载同一发布、安装和执行无窗口诊断通过，记录位于../installer；这补充安装/CLI证据，不替代未完成的下载版可见窗口检查。
