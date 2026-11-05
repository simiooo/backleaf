# SIM-1006 · 容量展示验收

实现源码 b006987b240658bbe3c3ab70a4e733a5405e3504，版本 0.1.0-preview.21。OpenPencil v42 保留原247页，新增六态；独立冷重开六页均0像素差。设计语义和尺寸说明见 ../../capacity-v42.md。

本机 fmt、严格 release/all-target Clippy、91项非无头测试与29项生产 GPUI 无头回归通过。格式化测试包含 B 至 YB、零值、单位边界进位、无多余 .0 和 u128 极值；容量更新原有无头用例检查固定侧栏几何与过期通知。日志与结构化报告保留原始字节。

2026-11-05 本地 release EXE SHA256 为 1b3e1026cc28279622b49870c40faab717554625a0cb976dc533406df9221042。本地 ZIP 解包部署至隔离 staging，安装后摘要一致并通过 CLI 冒烟，详见 local-staging.json；该打包脚本的 interactive_smoke_passed:false 表示其本身未执行交互检查。

另以相同 EXE 启动独立状态目录和7项自有夹具，Computer Use 检查真实 GPUI 图像：容量副文案小于标题、文本和静态细条共用左轴、GB标签不带多余 .0、两分区行高保持不变。可访问名称仍包含总量、已用、剩余和当前用户可用量。记录见 native-observation.json 与 native-process.json；检查后关闭该测试实例，重新列举确认已退出，没有关闭用户正常使用的实例。

小单位、未知容量、低剩余和深色状态通过设计稿及逻辑/无头测试核对，没有修改实际磁盘制造这些状态。此次不重复声称物理热插拔、NVDA、IME、OLE 或 GPU 性能全矩阵通过。

公开 Windows 包工作流42015919472与独立无头工作流42015916332均成功，均对应上述源码。[测试包](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.21-b006987b) 于2026-11-05 19:12:04+08发布；ZIP SHA256 abf5f95f4ce122edfe55cb00407a7c056564e9017fbefa41c4eb5b28020a1a57。CI EXE SHA256 4c403aec5fc01be5a90c01220f31680759758b07056cccdd05e8bfb3022fd196，和本机构建分别记录，不混用。

2026-11-05 19:12:49+08通过匿名公开下载路径实际安装到隔离 staging，发布/内部摘要一致，三个运行库均从应用目录加载，已安装EXE诊断通过。ci-install.json 中 gui_smoke 的固定描述仅表示该安装探针不执行 GUI 检查，不能解读为本版没有界面改动；真实窗口检查属于前述本地EXE。Release同时附可编辑 v42 .fig，摘要与冷重开设计稿一致。
