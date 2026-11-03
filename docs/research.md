# Research and decisions · retrieved 2026-11-03

Only first-party technical documentation informs the baseline. This is engineering research and design review; it is not a user interview or comparative performance study.

| Evidence | Consequence |
| --- | --- |
| [GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md) | Windows is a supported Win32/DirectWrite backend; API is pre-1.0. Pin a commit and Cargo.lock. |
| [Published GPUI 0.2.2](https://docs.rs/gpui/0.2.2/gpui/) compared with local upstream source | The published crate differs materially from current main; version number alone is insufficient. |
| [AccessKit merge #56065](https://github.com/zed-industries/zed/pull/56065) and upstream StatefulInteractiveElement source | Use an upstream commit containing AccessKit. Explicit roles, labels, selected states and focus are still the application's job. |
| [IFileOperation](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ifileoperation) | Use native Shell operations on an STA, set owner, capture item results and check abortion. Never infer completion from PerformOperations alone. |
| [Fluent motion](https://fluent2.microsoft.design/motion) | Quick, functional transitions; large content changes fade together. Durations in our design are hypotheses to test. |
| [SQLite FTS5](https://www.sqlite.org/fts5.html) | Trigram MATCH does not cover one/two-character substrings. Preserve exact Unicode substring verification and test short Chinese queries explicitly. |
| [Noto Sans SC OFL](https://github.com/google/fonts/blob/main/ofl/notosanssc/OFL.txt) | Font distribution includes OFL, avoiding a paid or restricted font dependency. |
| [Windows custom frames](https://learn.microsoft.com/en-us/windows/win32/dwm/customframe) and pinned GPUI Windows `WindowControlArea` hit-test implementation | Integrate navigation into custom chrome while keeping drag and control hit tests distinct. HTMAXBUTTON retains the platform's maximize behavior. |
| [Windows rounded corners](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-rounded-corners) | Ask DWM to round restored windows; maximized/snapped windows follow system policy. No region or per-pixel-alpha top-level window. |
| [OpenPencil 0.17.0-beta.1 release](https://github.com/open-pencil/open-pencil/releases/tag/v0.17.0-beta.1) | Upstream documents fixes for saved glyphs reopening as other letters. Use an isolated pinned runtime and independent file round-trip checks instead of treating an earlier PNG as proof of persisted editable content. |
| [GNOME progress](https://developer.gnome.org/hig/patterns/feedback/progress-bars.html), [popovers](https://developer.gnome.org/hig/patterns/containers/popovers.html) and [notifications](https://developer.gnome.org/hig/patterns/feedback/notifications.html) | Measured progress or unknown-total activity, compact attached operations, aggregated results. Exact sizes are our desktop choices. |
| [GNOME Files progress indicator](https://gitlab.gnome.org/GNOME/nautilus/-/blob/main/src/resources/ui/nautilus-progress-indicator.blp), [progress row](https://gitlab.gnome.org/GNOME/nautilus/-/blob/main/src/resources/ui/nautilus-progress-info-widget.blp) and [conflicts](https://gitlab.gnome.org/GNOME/nautilus/-/blob/main/src/resources/ui/nautilus-file-conflict-dialog.blp) | Shared operations popover, independently cancellable tasks, apply-to-remaining conflicts. Research only; no Nautilus implementation code copied. |
| [Finder organization](https://support.apple.com/guide/mac-help/organize-your-files-in-the-finder-mchle9f0a1b2/mac) and [drag and drop](https://support.apple.com/guide/mac-help/drag-and-drop-items-mh35852/mac) | Behavioral and hierarchy reference while retaining Windows shortcuts and Shell semantics. Center icon-grid identities as explicitly requested by the user. |

Windows staging means a private downloadable desktop build installed into an isolated staging directory and launched on Windows. A webpage screenshot or source push alone does not count as application deployment. Signed production distribution and default Shell takeover are outside this staging authorization.

## 大规模处理补充 · 2026-11-03

- [IFileOperation UpdateProgress](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-updateprogress) 是无指定单位的工作量估计点，不是字节；修正 v4 示例的 MB 表示。
- [CopyItems](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-copyitems) 可接受 Shell 数组/数据对象/枚举器，但声明工作不等于执行。限制应用持有对象数量，Shell 递归内存仍须实测。
- [Shell clipboard formats](https://learn.microsoft.com/en-us/windows/win32/shell/clipboard#cf_hdrop) 要求 CF_HDROP 完整路径数组，不能将应用虚拟选择宣传为系统剪贴板恒定内存。
- 针对当前固定上游版本，已读本地 gpui/src/elements/uniform_list.rs：只为可见范围调用 render_items。网格按行虚拟化，实际 UIA 与焦点另验。
- [GNOME progress](https://developer.gnome.org/hig/patterns/feedback/progress-bars.html) 支持按任务单位反馈和合理合并子任务；采用统一队列和按需明细，具体密度与上限是本产品选择。

## v6 文本与可见渲染

[GNOME tooltips](https://developer.gnome.org/hig/patterns/feedback/tooltips.html) 提供内容补充信息的参考。按用户偏好，任务列表隐藏常驻状态标签；完整名称/状态同时保留在详情与可访问信息，不只依赖悬停。已核对本地固定GPUI源码的 uniform_list、ShapedLine 与 shape_line 接口，可见范围和少量缓存绘制是实现候选；没有据此声称已完成GPU或UIA实机验证。
