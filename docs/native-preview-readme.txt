拾径 / Backleaf 0.1.0-preview.2

双击 Backleaf.exe，进入当前 Windows 用户 Home。
单击选择；Ctrl/Shift 多选；Ctrl+A 选择当前快照；方向键、Home/End、PageUp/PageDown 导航。
空白处按住左键拖动可框选，移到边缘自动滚动；Ctrl/Shift 框选追加选择，Esc 撤回当前框选。
Ctrl+方向键只移动键盘焦点，Space 切换焦点项，Shift 延伸选择。Ctrl+Space 保留给输入法。
双击或 Enter 打开目录/文件；Alt+Enter 查看完整详情；右键在本预览中也打开详情。
详情支持鼠标选择、Ctrl+A/Ctrl+C 和复制完整名称/路径。Esc 关闭详情并返回网格。
Alt+左箭头后退；Alt+上箭头进入父目录。

渲染样本：Backleaf.exe --fixture 1000000 --flow 8
样本中的文件名与进度是明确标示的合成压力数据，不会执行文件操作。
F6 打开/关闭样本面板。减少动画分支：追加 --reduced-motion。

本预览交付 GPUI 虚拟渲染、真实目录快照、长文本与文字流光。
新增统一明暗主题、选中/失焦/键盘焦点、共享间距与圆角，以及压缩海量选区。
搜索、快捷访问、复制/剪切/删除/重命名、常驻索引、恢复与完整系统集成仍在后续研发范围。
正常目录中不会出现虚构的任务进度。目录快照暂不自动刷新，重新进入目录会重新枚举。
第三方许可位于 licenses。性能方法、原始证据和已测范围见仓库 docs/native-render-validation.md。
