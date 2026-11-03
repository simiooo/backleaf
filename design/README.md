# 当前设计稿 · v6

在 OpenPencil 打开 **[Backleaf.fig](Backleaf.fig)**。这是唯一当前主稿，含 44 个可编辑页面；对应预览位于 [v6](v6/)。旧的独立 `.fig`、根目录 PNG、`v2`、`v3`、`v4`、`v5` 和 `v5.1` 是保留的历史稿，不是当前实施基线。

本轮放宽间距，容器上下大于左右；任务面板统一左轴并精简信息。文件图标与名称继续居中。

| 常看页面 | 预览 |
| --- | --- |
| Home / 44 px 窗口栏 | [home](v6/home.png) |
| 多选与菜单 | [04](v6/04-multiple-selection.png) |
| 行内重命名错误 | [06](v6/06-rename.png) |
| 560 px 窄窗 | [07](v6/07-narrow.png) |
| 六文件剪切待粘贴 | [15](v6/15-cut-pending.png) |
| 多操作同时处理 | [25](v6/25-multiple-operations.png) |
| 批次逐文件结果 | [26](v6/26-batch-item-results.png) |
| 多文件重名冲突 | [27](v6/27-name-conflicts.png) |
| 百万目录全选与排除 | [31](v6/31-million-directory.png) |
| 24项任务汇总 | [32](v6/32-large-queue.png) |
| 十万文件逐项结果 | [33](v6/33-large-task-detail.png) |
| 一万失败与重试 | [34](v6/34-large-failures.png) |
| 大量选择准备剪贴板 | [37](v6/37-large-clipboard.png) |
| 长文件名省略与tooltip | [40](v6/40-long-task-tooltip.png) |
| 完整名称与路径详情 | [41](v6/41-full-details.png) |
| 流光关键帧 | [42](v6/42-text-motion.png) · [运动预览](motion/filename-flow.svg) |

13–24 覆盖回收、进度、取消等待、部分结果、失败、重命名提交/完成和撤销；29 覆盖总量未知的准备过程。

尺寸与交互见 [设计契约](../docs/design-contract.md)。[布局审计](spacing-audit.json)、[保存结构检查](recovery-verification.json)、[独立重开视觉检查](cold-reopen-verification.json) 与 [精确文字检查](text-reopen-verification.json) 记录持久性和布局证据。文字逐项精确一致；视觉检查保留实际像素差及既定容差，不将细微栅格差异描述为内容丢失。

源文件为 [layout.mjs](../scripts/design/layout.mjs)，大规模状态源为 [bulk.mjs](../scripts/design/bulk.mjs)，逐页 JSX 与变量位于 [source](source/)。重建说明见 [scripts/design](../scripts/design/README.md)。当前交付物是设计稿，应用开发和 staging 安装验证仍未完成。
