# preview.15 · SIM-992

本批次沿用233页OpenPencil v38视觉/交互基线，补齐生产Root、TextInput、MenuPopup、真实隔离目录/搜索/文件服务的16项GPUI无头交互回归，另保留81项逻辑/文件/COM协议测试（共97项）。用例和原生边界矩阵见[回归分层](../../native-resume-checklist.md)。无头只替换HWND/显示器边界，不通过桌面输入操作应用；字体为合成度量，未启用像素渲染或屏幕阅读器适配。

mutation-verification.json与mutation-*.log记录反向验证：临时禁用TextInput Blur订阅后，Tab切换焦点的保存用例按预期失败；在MenuPopup render重新读取owner实体后，菜单重命名入口按原始路由问题失败。两处源码均逐字节恢复。普通空白点击会走网格自己的提交路径，不能单独证明输入框Blur订阅存在，因此主失焦用例使用Tab，另测空白点击和切换右键。

回归发现并修复：已有同名目标时，内联重命名会进入Shell冲突等待。后台先核验目标并返回错误、保留草稿和焦点，不覆盖源/目标；另测仅改变大小写仍成功。错误提示检查保留的内容、焦点与实际绘制的浮层几何，不把TestPlatform未激活的无障碍树冒充为UIA证据。

GitHub新增PR/staging/main相关推送的只读无头门禁；发包流程也执行相同门禁，检查干净源码与日志摘要，包内携带headless.json。测试失败、未运行足量用例或报告不匹配时拒绝发布。最终本机/CI构建、native-only验证、发布与下载核对的版本和摘要会分别追加，不混用不同EXE的证据。
