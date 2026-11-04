# SIM-954：菜单与文件操作

当前为实施中的设计与验收约束。已发布 preview.2 不包含这些功能。可编辑稿是 [Backleaf-v10.fig](../design/Backleaf-v10.fig)，保留 v9 的 52 页，新增 53–70 页。18 页均完成独立冷重开；[几何检查](../design/v10/geometry-audit.json)核对实际图层，而不是生成代码的意图。

| 上下文 | 动作 |
| --- | --- |
| 单文件 | 打开、复制、剪切、复制路径、重命名、移至回收站、详情 |
| 单文件夹 | 同上，增加粘贴到文件夹 |
| 多选 | 复制、剪切、复制路径、移至回收站；本轮重命名只支持一个项目，禁用原因放 tooltip |
| 空白 | 新建文件夹、粘贴到当前位置、刷新；不沿用先前选区 |

右击已选项目保留多选；右击未选项目只选该项目。空白菜单清除选区。Shift+F10/菜单键锚定焦点项目，没有项目时打开空白菜单。方向键/Home/End/Enter/Esc 和焦点返回必须可用。菜单行 32 px、半径 8，外层上下 8/左右 4、半径 12；保持 v9 的交互覆盖层与明暗语义色。

Ctrl+C/X/V、F2、Delete/Ctrl+D、Ctrl+Shift+N、F5、Ctrl+L/Alt+D、Ctrl+Shift+C 依焦点分发。文本输入和 IME 组合优先，不能触发文件删除、粘贴或重命名提交；系统 Alt+Space 等快捷键继续保留。菜单里的“粘贴到文件夹”使用菜单捕获的目标；菜单外 Ctrl+V 使用当前目录。

重命名在原网格单元内编辑，选择主文件名，保留扩展名。错误保留输入，提示不改变固定行高；完整名称可横向浏览。新建后进入同样的编辑流程。名称验证拒绝路径分隔、空名、点目录、保留设备名和无效尾字符；实际冲突和权限仍交给结果核对。待剪切图标淡化，名称保持可读；按实际移动结果清除，外部剪贴板改变和 Esc 取消剪切会同步更新。

任务只在窗口栏有一个入口，队列最多 24 项，保守串行执行；队列视口不超过 360 px，详情不超过 380 px。总览仅有目标、进度、项数和大小。两列计数固定左轴，所有文件名、目标和进度条共用内容 x12；容器上下 16/左右 12。进行中只有取消，等待实际停止时显示“取消中”；结束后才换成关闭。Esc/点击外部收起，任务继续。关闭最后窗口时有任务则给“继续处理 / 取消任务并退出”，取消完成后才能退出。

结果明细按游标分页、只渲染可见行，不加状态图标或行尾文案。正文/灰/蓝色流光/红色分别表达完成、等待或取消、进行、失败，准确状态与完整长名称放 tooltip、可访问名称和详情。只重试已确认失败项，成功和不确定结果不自动重放。取消保留已完成结果。

文件选择在命令边界固定为不可变快照，后台逐批写入 SQLite 清单，不在 GPUI 线程遍历百万路径。路径以 UTF-16 字节保留；按条数及编码字节双重限制批次，终态入库。剪贴板使用 Windows 标准文件格式，预估负载，沿用 256 MiB 上限；超限或准备失败保留旧剪贴板，明确让用户减少选择。不偷偷替换成仅本应用能粘贴的私有格式。

Windows 原生适配须先做小规模真实验证，再接入应用。所有 Shell 对象留在创建它们的 STA。操作分批声明和执行，避免积存百万对象；取消使用独立信号；在每个操作前核对已准备的文件身份。同名替换须来自具体决策，回收失败不降级为永久删除。

Microsoft 区分了进度点数与字节/项数：[IOperationsProgressDialog::UpdateProgress](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ioperationsprogressdialog-updateprogress) 提供三种独立度量，可经 [SetProgressDialog](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setprogressdialog) 接入。总量未确定时显示“—”，不阻塞首屏全树扫描，不将普通 sink 的工作量点换算为字节。最终逐项结果还需核对 [GetAnyOperationsAborted](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-getanyoperationsaborted)，仅 PerformOperations 返回成功不足以证明完成。

交付条件：数据正确性与取消/冲突/失败隔离夹具、Explorer 剪贴板互操作、GPUI 可见菜单/输入/快捷键冒烟、有界资源检查、源码推送、实际 staging 安装及冒烟、版本/哈希/时间/证据。达到后 SIM-954 才进入产品待验收；搜索、统一快捷访问、索引和恢复继续原单。
