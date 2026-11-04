# SIM-953 验证证据

源码 `d28e5af81f787fb8257cdce1d484caac667e30c9`，版本 `0.1.0-preview.2`。正式原始报告来自该源码生成并安装到独立 staging 的同一 EXE；旧候选和锁屏诊断保留在 `.local`，没有混入本目录。

- [staging.json](staging.json)：安装/发布状态、源版本、EXE 和 ZIP 哈希、时间。
- [build.json](build.json)：编译器、源输入哈希、固定 GPUI 版本和检查日志哈希。
- [manifest.json](manifest.json)：逐份证据 SHA256。
- [summary.json](summary.json)：13 组原生验证摘要，零样本时间使用 null。
- [reports](reports/)：11 组元数据渲染、实际 100,001 项目录和控制器集成。
- [ui-smoke.json](ui-smoke.json)：实际窗口动作、UIA、系统菜单、长文本和恢复检查。
- [copy-name.json](copy-name.json)、[copy-path.json](copy-path.json)：合成样本完整剪贴板值相等检查。
- [hardware.json](hardware.json)：本机硬件枚举，不证明 DXGI 选中了哪张适配器。

截图为测试窗口的原始捕获，可能包含自动化工具的鼠标提示。`short-selected.jpg`、`dark-flow.jpg`、`long-details.jpg`、`ten-million.jpg` 记录内容包裹背景、深色流光、完整详情和千万项末尾。

CPU 场景时间不是 GPU 呈现或输入到光子延迟；合成任务进度不执行文件复制。控制器脚本在真实 GPUI 窗口中驱动事件和重绘，不冒充操作系统指针长按。门槛沿用 [渲染契约](../../native-render-validation.md)，结果见 [选择验证](../../native-selection-validation.md)。
