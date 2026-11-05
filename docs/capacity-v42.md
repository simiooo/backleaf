# SIM-1006 · 容量文案与静态用量

调研采用静态meter语义：磁盘占用表示已有用量，不播放任务进行动画。微软文档明确以磁盘占用举例区分meter；该指南年代较早，仅采用语义、紧邻标注和共同左轴的原则，外观沿用Backleaf现有基线。[微软meter说明](https://learn.microsoft.com/en-us/windows/win32/uxguide/progress-bars#meters)。GNOME的LevelBar同样用于已有水平值，具备meter无障碍语义。[GTK LevelBar](https://docs.gtk.org/gtk4/class.LevelBar.html)。

OpenPencil v42保留247页，新增六态：就绪、未知、小单位、极少剩余、选中和深色。分区标题保持原尺寸，容量副文案10px/14px，文字和2px细条合为18px高的一个静态组件，共用同一左轴与宽度；没有额外卡片、百分比或状态图标。分区行仍60px，数据变化不移动其余导航。

显示例如“75.1 GB 剩余 · 400 GB”。B/KB/MB/GB/TB等单位随数值变化，零为0 B，不必要的.0移除，四舍五入达到1024时提升单位，避免1024 KB。按Windows文件管理器和现有任务面板习惯保留1024递进、使用常用KB/MB/GB标签；这属于Windows式显示约定，不把KB标签说成严格SI千进制单位。未知为—，不编造0或剩余量。tooltip和可访问名称继续保留总量、已用、剩余、配额可用信息。

字节汇总保持u128，不转换为浮点数后参与总量或边界判断。容量、选择与任务大小使用同一格式化函数；原始数量与文件系统操作不改变。
