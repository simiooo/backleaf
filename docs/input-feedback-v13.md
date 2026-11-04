# v13 输入层级、失焦提交与导航反馈

2026-11-04 产品反馈覆盖此前 v11/v12 的 hover 下拉、文件名字重与输入焦点方案。可编辑基线为 [Backleaf-v13.fig](../design/Backleaf-v13.fig)，保留 92 页并新增 9 页，明暗主题共 101 页。

- 搜索层只消费自己的导航操作；普通字符、文本编辑快捷键和输入法交给原生输入框。此前 UIA SetValue 通过并不能证明真实键盘路径可用，补做数字键、字母、Ctrl+A 与中文候选提交检查。
- 面包屑外容器高 36、四周 4，子项高 28、内边距 4；外层中性表面与子项 hover 分开。移除面包屑 tooltip 和 hover 自动下拉，点击箭头展开目录，省略号点击展开折叠祖先。保留祖先链接、双击编辑和键盘操作。
- 文件名常规字重保持度量一致；只在文件名区域提供完整名称 tooltip，图标与卡片留白不触发。
- 搜索、路径和重命名共用同一输入框：32 高、8 圆角、恒定 1 px 中性边界；激活时使用 input-active 表面与接触/环境阴影，非激活使用 input-rest，不用亮色粗边框区分。明暗表面由 `color-system.json` 语义变量提供。
- 重命名在 Enter 或失焦时提交。输入法预编辑先完成组合，避免提交半个候选；错误保留值并恢复焦点。重命名后的右键菜单、路径跳转、搜索和关闭通过一个最新意图槽接续；菜单在目录刷新后以原始 UTF-16 名称定位目标，避免沿用旧行号。仅成功后执行接续操作；失败保留编辑器。只读分页新增原始名称字段，仍按可见页加载。

Meta 的公开 [ColorScheme](https://developers.meta.com/horizon/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_colorscheme/) 区分背景、表面、变体与交互角色，要求表面和内容颜色成对；[无障碍设计文章](https://www.meta.com/design-at-meta/blog/5-ways-to-integrate-accessible-design-into-fast-paced-projects/) 强调多种视觉线索、焦点顺序和键盘使用。本项目据此及用户指示设计桌面层级，不把自己的尺寸、RGB 或“输入框必须加阴影”宣称为 Meta 的通用规范或已核实的最新趋势。

实际 Windows Shell 拒绝某些含 `/` 的 DOS 解析路径；适配层在调用 Shell 前统一分隔符，并在解析后复核文件身份，不放宽设备命名空间、末尾点/空格及源文件身份保护。依据 [Microsoft 文件路径约定](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)，并通过同一文件先使用规范路径、再用混合分隔符重命名、内容和账本校验验证。

发布脚本改为先建草稿、逐资产重试上传、逐项校验服务端摘要后公开。原 CI 的构建和解压冒烟成功，但发布遇到 GitHub 502；该有输入缺陷的 preview.4 未完成发布。本次以 preview.5 重新构建验证。保留只恢复已有合格 CI 产物的独立工作流，不在恢复阶段执行下载的程序。
