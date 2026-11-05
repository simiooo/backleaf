# 外部拖放与自绘扩展菜单批次

2026-11-05：用户要求继续补齐外部/跨窗口拖放，并把第三方注册菜单项迁入Backleaf自绘菜单，最后统一检查交付。唯一执行位先SIM-974，实施完成后再转SIM-978；中途源码不是交付。

设计：OpenPencil v37，保留前221页并新增222–230；v38再新增独立owned菜单窗口/子菜单/焦点返回，233页冷重开通过。复制/移动、跨窗、外部拖入/拖出、侧栏固定、无效目标；扩展项与动态子菜单使用同一自绘样式。宿主仅用于发现/消息转发/调用，不弹原生菜单。可访问名称、禁用/勾选、键盘上下左右/Enter/Esc和焦点返回沿用。

拖放架构：原生OLE取代固定COPY的上游drop target，接收CF_HDROP真实磁盘文件；同进程跨窗优先使用随机会话令牌和冻结选区，避免百万路径进入UI。外部数据对象在后台STA解包为落盘清单，目标与身份/物理目录环在后台核实，最终复用任务协调和IFileOperation。Ctrl复制、Shift移动，默认按卷关系；链接和虚拟内容格式明确拒绝。拖出数据对象延迟生成CF_HDROP，保留完整UTF16、身份和明确资源预算，不截断选区。DoDragDrop在脱离GPUI更新借用后进入原生消息循环，后台STA提供数据。异步源用IDataObjectAsyncCapability协商及完成通知；同步源等待实际结果并泵送消息。原应用拥有的文件不凭提前的MOVE效果盲删。

扩展架构：隔离STA宿主保留IContextMenu和冻结目标，按需初始化动态子菜单，传回稳定条目标识、文本、状态和层级。自绘菜单按用户选择请求命令；调用前再核源身份。标准Shell动作与已有动作去重，扩展不自动执行。无法提取文字的owner-draw项明确不可用，不伪造为已兼容。超时/失败仅终止尚未调用命令的发现会话，执行中的第三方命令不被伪造取消。

最终验证：真实Explorer↔Backleaf、Backleaf窗间、Ctrl/Shift、取消、目标关闭/替换/目录环、多项与大选区；具体已安装扩展的单/多项、动态子菜单与无破坏命令；输入/滚动/海量性能回归。之后统一fmt/Clippy/tests/build、staging、CI及下载核验。未运行不标通过。

依据：https://learn.microsoft.com/en-us/windows/win32/shell/datascenarios ，https://learn.microsoft.com/en-us/windows/win32/api/oleidl/nf-oleidl-idroptarget-dragenter 。

菜单窗口：GPUI PopUp建立后绑定源窗口owner，取消全局topmost/taskbar，透明无边框；文件/路径/侧栏入口统一使用。目录画布不再承载菜单实体，窗口工作区翻转/约束大小，父窗口生命周期与菜单entity绑定。菜单窗口不计入最后一个文件窗口的任务关闭保护。菜单项统一32px行、4/8px内边距、12px勾选槽+8px间距，文字32px起线，无加粗位移。

统一检查进展：81项Release已通过，含真实Shell IDataObject/CF_HDROP往返、原生IDropTarget同步/异步到IFileOperation的实际移动、PerformedDropEffect=NONE避免二次删除。GUI已实测独立菜单/动态子菜单/Esc焦点及选区保留，7-Zip测试1压缩包/1文件30字节和2压缩包/2文件60字节均无错误；同窗真实快速OLE移动与1/1项、20/20B进度。工具拒绝源窗口边界外的拖动终点，未绕过，也不把跨窗/外部手势当GUI已通过。

修复：owned popup保留WS_VISIBLE，原生尺寸改变后显式同步GPUI bounds_changed避免旧viewport裁切；菜单高度至多640逻辑px/屏幕工作区，窄屏只展示可容纳的最近层级。OLE启动检查App/Window借用状态，目标回调只做受保护的viewport几何更新，磁盘/身份仍后台处理；短等待不消费鼠标/键盘消息，源同时读取实际鼠标释放，避免拖动停留。侧栏原生固定保留before/revision、原插入提示线和边缘滚动。

归档 staging 冒烟发现 fd5368c 的菜单后续动作焦点错误，已取消 CI 41973622289，不作为交付。根因是 popup render 读取/观察 Backleaf 主实体，GPUI 将 current_window_by_entity 改指弹窗；后续订阅和 defer 随弹窗关闭而丢失。改为 OwnedMenu sender 租约和 receiver 断开通知生命周期，弹窗仅访问菜单实体；命令延后回主窗口执行。实机验证压缩输入自动聚焦、失焦提交成功（1/1 项、42/42 B，ZIP 内容逐字节一致），路径菜单进入编辑后失效路径 toast 并恢复草稿/焦点，重命名菜单自动聚焦与失焦保存成功。截图见 preview-14/owned-*，与此前阶段分开记录。
