# SIM-965 窗口栏与搜索动效

OpenPencil v16 120页、v17 124页，分别保留此前全部页面。v17补充收起起点、25%时间、窄窗尾段和图标终点；冷重开图层及像素容差检查通过。

原生窗口栏48px，控件32px，垂直留白8px，按钮间距8px；hover/focus阴影分别3.5/4.5%和5.5/6.5%，位移1/2px。图标/输入行居中，两列数据基线；外层纵向结构仍顶部排列。

搜索展开240ms、收起200ms，cubic缓出，反向从当前插值继续。关闭即时取消查询、移除输入实体和结果层；残留的只读文字随宽度淡出。重复关闭不重启动画，减少动态效果直接跳到终点，复用一个有限窗口动画时钟。

38项Release测试通过；包括完整展开/收起的单调有界终点、中途反向、重复关闭和减少动态效果。Clippy -D warnings、fmt、Release构建通过。原生候选cdbf640ed77759011d1fff6d61669ccead6495d328379dcccc9721ff83cd4580于2026-11-04 22:28:27+08部署至隔离staging，CLI会话预检未锁定。

raw/search-open.png、search-closing.png、search-closed.png为同一真实GPUI候选的中文查询、Esc收起中间帧与图标终点；外部点击及双击路径编辑也捕捉到收起中间帧。终点UIA确认输入框已移除、焦点返回应用。type_text验证中文文本通道，不等同于本轮重新测过全部输入法候选组合；此前preview.6键盘/IME证据另行保留。

最终性能、源码映射和CI包验证记录在后续JSON，未发布前不把本机候选称为已发布包。

产品于2026-11-04明确反馈“关闭动画我看过了，没问题”，本轮收起视觉已获确认；其他整体审美与功能不由此推断完成。深色560px窄窗展开/收起中间帧另有截图；路径按可用宽度省略。

最终本机12s样本：1024个自有EXE夹具滚动，scene CPU p95 3.965ms，图标LRU128/队列32/结果峰6，已提取1561次覆盖淘汰；1000万合成元数据+8个流光任务滚动，scene CPU p95 4.372ms，可见格峰50、目录缓存峰90、工作集峰72.85MiB。回调间隔不是GPU呈现延迟；样本不等同于真实复制1000万文件。原始报告记录会话未锁定。

2026-11-04侧栏追加：v18 129页冷重开检查通过。移除可见标题，仍保留导航的辅助功能名称；Tab focus使用实色表面、细中性色边缘及淡阴影，避免透明底把零模糊阴影内部暴露成蓝色块。真实Shift+Tab从网格进入视频行已查看并保存sidebar-tab-focus.png，Tab返回网格后侧栏样式恢复。新候选SHA及部署入口见sidebar-staging.json；之前关闭动画与性能截图保持各自候选哈希，不混称同一二进制。

最终框选控制器回归通过：此前固定850ms假设未确保窗口在前台且跨过3968行重定位阈值，诊断记录出现false。测试改为等待实际激活、从生产阈值行开始并等待真实重定位，保留原生产阈值不变；selection-controller.json中系统快捷键保留、边缘滚动、rebase、Esc撤回、追加选择全部true。它是原生窗口内的合成控制器事件，不冒充OS鼠标拖动。

最终sidebar候选30971793重新执行同两组12s压测并通过全部有界断言：1024-real-exe-scroll scene CPU p95 4.345ms，工作集峰79.33MiB；10m-scroll-flow8 scene CPU p95 5.112ms，工作集峰74.90MiB。最终报告为final-*.json；旧cdbf报告保留作阶段记录。

最终30971793候选的减少动态效果模式也经真实Ctrl+F/Esc检查：直接展开/收起终点，输入焦点返回应用；45秒原生报告reduced_motion=true、animation_requests=0、hidden_animation_requests=0。截图和报告位于raw/reduced-motion-*。

## 工程交付

preview.7已于2026-11-04 22:52:59+08发布：[测试包](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.7-4dff8618)。源码`4dff861841899f63a6fdc234c021efc4147f77b5`，CI [41850944339](https://github.com/simiooo/backleaf/actions/runs/41850944339)成功，38测试通过，解压后CLI冒烟通过。下载Release四项资产的服务端digest、CI artifact与Release逐字节对比、ZIP内EXE/Logo/可编辑v18设计及源码版本一致性检查均通过。ZIP SHA256 `e265bd034d5fa244a7e188f9895140fb4c010affa3e25f5cfcc620f9503421b7`。

本机可见冒烟/性能使用可信本地构建30971793，CI EXE为db4edd61，分别记录，未执行下载的远端EXE。CI不作为可见桌面测试；CI构建清单与本机输入清单保留。外部CRUD自动同步、保存搜索、全局持久索引与启动恢复未包含在本次完成范围。
