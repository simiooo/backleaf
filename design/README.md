当前版本：[Backleaf-v38.fig](Backleaf-v38.fig)，233页。v37补齐OLE拖入/拖出、跨窗、侧栏固定及自绘扩展菜单；v38补齐独立owned菜单窗口、动态子菜单与焦点返回。冷重开验证：[v38/cold-reopen.json](v38/cold-reopen.json)。所有此前页面和历史文件均保留。

最新主稿：[Backleaf-v22.fig](Backleaf-v22.fig)，149页，保留已发布v20的143页，新增6个渐进框选状态。持续加速的参数和行为见[交互契约](marquee-scroll-contract.md)。v21路径/磁盘设计仍属于另一个未交付候选单元。

最新主稿：[Backleaf-v20.fig](Backleaf-v20.fig)，143页；保留前137页，补充短/长名称、空白清选、重命名输入与明暗/窄窗命中状态。

最新主稿：[Backleaf-v19.fig](Backleaf-v19.fig)，137页；8个紧凑计数/无溢出/主区溢出/侧栏hover条件/短窗/深色/千万/空列表状态，保留全部前129页。几何见[scroll-system.json](scroll-system.json)。

最新主稿：[Backleaf-v18.fig](Backleaf-v18.fig)，129页，保留此前124页并补齐侧栏normal、Tab焦点、当前目录+焦点、深色、窄窗5个状态。

本轮主稿：[Backleaf-v17.fig](Backleaf-v17.fig)，124页。v14/v15图标探索与生图原稿、v16窗口栏8个状态、v17搜索收起4个关键状态均保留。收起过程见[v17](v17/manifest.json)，有限双向动效契约见[search-motion.json](search-motion.json)。

当前文件类型图标基线：[Backleaf-v15.fig](Backleaf-v15.fig)，112 页。用户要求使用 image_gen 的 14 张透明原图，包含明暗、网格、小尺寸和 EXE 原生资源/回退状态；图案按可见范围归一到约 40 px，槽位仍为 44 px。容器与文字可编辑，图标是位图。v14 为保留的矢量探索，不作为本轮运行图标。详见 [图标规范](../docs/file-type-icons.md)。

当前交互反馈基线：[Backleaf-v13.fig](Backleaf-v13.fig)，101 页，保留 v12 的 92 页，新增 9 个输入、重命名与面包屑状态。预览与冷重开记录见 v13/。

当前搜索入口基线：[Backleaf-v12.fig](Backleaf-v12.fig)，92 页，保留 v11 的 84 页，新增 8 个搜索状态。预览与冷重开记录见 v12/。

# 当前修订稿 · v11

在 OpenPencil 打开 **[Backleaf-v11.fig](Backleaf-v11.fig)**，共 84 个可编辑页面。71–84 页加入左侧导航、Ant Design / shadcn 面包屑、浮层和 inline 选择计数，详见 [v11 导航规范](../docs/navigation-v11.md)。

历史 [Backleaf-v10.fig](Backleaf-v10.fig)，共 70 个可编辑页面。前 52 页完整保留 v9，53–70 页为正在实施的菜单与操作设计：[单文件](v10/53-file-menu.png)、[多选](v10/54-multi-menu.png)、[空白](v10/55-blank-menu.png)、[任务](v10/60-progress.png)、[队列](v10/65-bounded-queue.png)。详见 [SIM-954 约束](../docs/file-operations.md)。v10 不是已发布功能清单。

已发布 preview.2 对应 **[Backleaf-v9.fig](Backleaf-v9.fig)**，共 52 页。前 44 页保留 v6 流程，45–52 页为状态、颜色、间距和圆角修订：[明色状态](v9/46-light-states.png)、[深色状态](v9/47-dark-states.png)、[文件选择](v9/48-selection.png)、[任务面板](v9/51-task-light.png)。详见 [v9 规则](../docs/visual-system-v9.md)。原 [Backleaf.fig](Backleaf.fig) 是不变的 v6 存档，对应预览位于 [v6](v6/)。v7 已撤回，v8 是被 v9 替代的中间稿。旧的独立 `.fig`、根目录 PNG、`v2`、`v3`、`v4`、`v5` 和 `v5.1` 是历史稿。

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

源文件为 [layout.mjs](../scripts/design/layout.mjs)，大规模状态源为 [bulk.mjs](../scripts/design/bulk.mjs)，逐页 JSX 与变量位于 [source](source/)。重建说明见 [scripts/design](../scripts/design/README.md)。v9 对应的 preview.2 已完成原生实现、实机验证与 staging 安装冒烟，发布状态与记录见原生选择验证文档；视觉审美仍待产品验收。
