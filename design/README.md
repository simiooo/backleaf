# 当前设计稿 · v5.1

在 OpenPencil 打开 **[Backleaf.fig](Backleaf.fig)**。这是唯一当前主稿，含 38 个可编辑页面；对应预览位于 [v5.1](v5.1/)。旧的独立 `.fig`、根目录 PNG、`v2`、`v3`、`v4` 和 `v5` 是保留的历史稿，不是当前实施基线。

本轮延续紧凑中性风格，加入大规模任务与文件结果的有界面板。文件图标与名称继续居中。

| 常看页面 | 预览 |
| --- | --- |
| Home / 40 px 窗口栏 | [home](v5.1/home.png) |
| 多选与菜单 | [04](v5.1/04-multiple-selection.png) |
| 行内重命名错误 | [06](v5.1/06-rename.png) |
| 560 px 窄窗 | [07](v5.1/07-narrow.png) |
| 六文件剪切待粘贴 | [15](v5.1/15-cut-pending.png) |
| 多操作同时处理 | [25](v5.1/25-multiple-operations.png) |
| 批次逐文件结果 | [26](v5.1/26-batch-item-results.png) |
| 多文件重名冲突 | [27](v5.1/27-name-conflicts.png) |
| 百万目录全选与排除 | [31](v5.1/31-million-directory.png) |
| 24项任务汇总 | [32](v5.1/32-large-queue.png) |
| 十万文件逐项结果 | [33](v5.1/33-large-task-detail.png) |
| 一万失败与重试 | [34](v5.1/34-large-failures.png) |
| 大量选择准备剪贴板 | [37](v5.1/37-large-clipboard.png) |

13–24 覆盖回收、进度、取消等待、部分结果、失败、重命名提交/完成和撤销；29 覆盖总量未知的准备过程。

尺寸与交互见 [设计契约](../docs/design-contract.md)。[布局审计](spacing-audit.json)、[保存结构检查](recovery-verification.json)、[独立重开视觉检查](cold-reopen-verification.json) 与 [精确文字检查](text-reopen-verification.json) 记录持久性和布局证据。文字逐项精确一致；视觉检查保留实际像素差及既定容差，不将细微栅格差异描述为内容丢失。

源文件为 [layout.mjs](../scripts/design/layout.mjs)，大规模状态源为 [bulk.mjs](../scripts/design/bulk.mjs)，逐页 JSX 与变量位于 [source](source/)。重建说明见 [scripts/design](../scripts/design/README.md)。当前交付物是设计稿，应用开发和 staging 安装验证仍未完成。
