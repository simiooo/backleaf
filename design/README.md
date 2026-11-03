# 当前设计稿 · v4

在 OpenPencil 打开 **[Backleaf.fig](Backleaf.fig)**。这是唯一当前主稿，含 30 个可编辑页面；对应预览位于 [v4](v4/)。旧的独立 `.fig`、根目录 PNG、`v2` 和 `v3` 是保留的历史稿，不是当前实施基线。

本轮收紧容器与控件，去掉大面积彩色选中块，文件图标与名称继续居中。

| 常看页面 | 预览 |
| --- | --- |
| Home / 40 px 窗口栏 | [home](v4/home.png) |
| 多选与菜单 | [04](v4/04-multiple-selection.png) |
| 行内重命名错误 | [06](v4/06-rename.png) |
| 560 px 窄窗 | [07](v4/07-narrow.png) |
| 六文件剪切待粘贴 | [15](v4/15-cut-pending.png) |
| 多操作同时处理 | [25](v4/25-multiple-operations.png) |
| 批次逐文件结果 | [26](v4/26-batch-item-results.png) |
| 多文件重名冲突 | [27](v4/27-name-conflicts.png) |

13–24 覆盖回收、进度、取消等待、部分结果、失败、重命名提交/完成和撤销；29 覆盖总量未知的准备过程。

尺寸与交互见 [设计契约](../docs/design-contract.md)。[布局审计](spacing-audit.json)、[保存结构检查](recovery-verification.json) 与 [独立重开检查](cold-reopen-verification.json) 记录持久性和布局证据。

源文件为 [layout.mjs](../scripts/design/layout.mjs)，逐页 JSX 与变量位于 [source](source/)。重建说明见 [scripts/design](../scripts/design/README.md)。当前交付物是设计稿，应用开发和 staging 安装验证仍未完成。
