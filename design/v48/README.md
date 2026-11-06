# Backleaf v48 · Apple 式白/黑内容、蓝文件夹与悬浮 identity（设计草稿）

生成时间：2026-11-06；基于 `design/Backleaf-v46.fig`（271页）保留全部页面，新增 4 页，共 275 页。

## 本轮改动

- 颜色语义升级为 Apple 式分工（`design/color-system.json` v10）：
  - 浅色主内容为白 `#FFFFFF`，深色主内容为纯黑 `#000000`；
  - 灰色只用于标题栏/侧栏/分组 chrome，不再铺满内容面或文件夹；
  - 文件夹恢复蓝色身份色（浅色 `#BBD0E1/#58758D`，深色 `#5277A3/#B6D3F1`）；
  - 文件类型图标（Word/Excel/PPT/PDF/代码/压缩包等）保留自身识别色，不按主题色重着色；
  - 选中态采用方案 2：抬起 identity 面（radius 10）+ 蓝色文件名，不使用整格填充或描边。
- 悬浮层级改为更克制的 Apple 式阴影（`design/elevation-system.json` v2）：只有真正浮起的 identity/菜单/任务面板有阴影，普通内容面几乎平贴。
- 几何严格按现有 token：
  - 文件 identity/选中面 radius **10**，控件 radius **8**，浮动面/面板 radius **12**，文件主表面 radius **16**；
  - 文件单元高 **112**、图标 **44**、列 gap **8**、行 gap **16**；
  - 侧栏行高 **32**、gap **4**、paddingY **16**；
  - content/x **12**、content/y **16**、section **16**、inline/collection **8**、identity **6**、panel **12/16**。
- 新增 272/273 表面与层级板，274/275 浅/深色应用示意；既有 271 页完成颜色映射，几何、文本与节点数不变。

## 预览

- [272 浅色表面与层级](272-v48-apple-light.png)
- [273 深色表面与层级](273-v48-apple-dark.png)
- [274 浅色应用与选中态](274-v48-app-study-light.png)
- [275 深色应用与选中态](275-v48-app-study-dark.png)
- 既有页重着色抽样：[04 多选](after-04.png)、[08 深色](after-08.png)

## 验证

- 271 页保留页名称/节点数与 v46 一致；独立进程 `--export` 后再以另一个进程 `--verify`，6 个检查页面 0 像素变化，`cold-reopen.json` 为 passed。
- 设计稿可编辑；`manifest.json` 记录 master/变量文件 sha256。
- 已知限制：既有 271 页的旧几何中仍有少量边框/填充式状态，本轮只做颜色映射与新增示意页；逐页删除旧 outline、统一悬浮态在下一轮完成。
