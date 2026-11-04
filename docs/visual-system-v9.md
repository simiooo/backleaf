# 颜色、状态和几何 · v9

2026-11-04。SIM-953 当前设计与实施依据；此前 v7 已被用户否定，v8 的几何已被本稿替代。原 v6 的 44 页保持不变，v9 在可编辑原稿中增加八页。此文中的新数值是本次设计方案，尚未作为用户最终验收结论。

Meta 的公开资料支持语义颜色、容器与内容配对、交互覆盖层，以及按层级使用表面色；并不提供一个跨所有产品通用的 RGB 调色板。研究依据及推断分开记录在 [research.md](research.md)。主要依据为 [WhatsApp 的界面更新](https://www.meta.com/design-at-meta/blog/whatsapp-user-interface-update/)、[公开 ColorScheme](https://developers.meta.com/horizon/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_colorscheme/) 和 [InteractionOverlay](https://developers.meta.com/vr/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_interactionoverlay/)。XR 的空间尺寸没有直接移植到桌面应用。

| 元素 | 几何和作用 |
| --- | --- |
| 内容与详情区 | 横向 12、纵向 16 px；章节间距 16 px |
| 文件点击单元 | 最小宽 100、高 112 px，列间 8、行间 16 px；状态不改变排布 |
| 可见文件状态背景 | 图标与实际名称整体包裹，横向 8、上 8、下 12 px，圆角 10 px |
| 图标与名称 | 图标 44 px，中间 8 px；名称最多两行、行高 16 px，居中，溢出省略 |
| 控件 | 点击高度 32、圆角 8 px；主操作横向 16，次操作 12 px |
| 菜单 | 外圆角 12，横向 4、纵向 8 px；行高 32、行内横向 8、行圆角 8 px |
| 菜单文字实际边距 | 横向 12、纵向 16 px，与内容区呼应；分组断点 16 px |
| 浮层与任务面板 | 圆角 12 px，使用浮层表面和阴影；常规文件不加阴影 |
| 窗口外框 | Windows DWM 控制，兼容最大化和贴靠行为 |

重复发生在同类元素内部；文件单元的固定节拍、章节间更大的间隔、浮层更大的圆角、主操作额外的横向留白承担不同作用。短名称状态背景高 92 px，两行名称高 108 px，图标与名称首行保持同一基线。不能把点击热区的空白全部涂成选中背景。

默认文件没有卡片。选中时，图标与名称共用一个浅色状态层；不再拼接独立图标底板、实色名称底和整格边框。Hover、pressed 在当前表面上叠加中性色。键盘焦点是独立轮廓，仅在键盘导航时显示；失焦选中与不可用分别处理。明暗主题共用相同几何和 23 个语义颜色角色。

可编辑文件：`design/Backleaf-v9.fig`。源脚本：`scripts/design/visual-v9.mjs`。共享规格：`design/color-system.json`、`design/layout-system.json`。渲染、对比度、冷重开和实际节点几何证据均在 `design/v9/`。菜单页是后续 SIM-954 的视觉样本，本身不表示原生命令已完成。

原生交互已完成构建、实机验证、源码推送和 staging 安装冒烟。正式发布与验收状态见 SIM-953 和原生选择验证文档；本稿没有被标作用户已完成审美验收。
