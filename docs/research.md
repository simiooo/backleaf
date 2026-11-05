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

## Meta 颜色与状态系统调研 · 2026-11-04

用户指出 v7 的灰色图标底板和独立蓝色名称底缺少统一层级，要求先调研 Meta，再按浮层层级、重复与节奏重做状态系统。v7 配色因此撤回，尚未发布。

| 一手来源与适用范围 | 可核查结论 | 对 Backleaf 的设计推导 |
| --- | --- | --- |
| [WhatsApp 设计更新，2024-05-09](https://www.meta.com/design-at-meta/blog/whatsapp-user-interface-update/) | 采用统一强调色系，增加中性色；图标使用一致的圆润描边语言 | 大面积以中性色承载内容；同一类状态重复同一语义，避免图标与名称各配一套底色 |
| [Facebook 品牌更新，2023-09-20](https://www.meta.com/design-at-meta/blog/redefining-facebooks-brand-identity/) | 核心蓝与扩展明度范围共同使用，并考虑对比度和图标可读性 | 强调色需要成套明度关系；不能把品牌蓝直接铺在每个状态上 |
| [Meta UI Set ColorScheme 1.0.0](https://developers.meta.com/horizon/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_colorscheme/) | 区分 background/surface/surfaceVariant、accent/selected、状态、边界、交互及进度；容器与前景成对 | 表面、选择、焦点、反馈分工；先定义角色，再决定数值 |
| [InteractionColors](https://developers.meta.com/vr/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_interactioncolors/) 与 [InteractionOverlay](https://developers.meta.com/vr/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_interactionoverlay/) | 中性、强调和媒体表面有相应叠层，hover/pressed 为交互覆盖色，scrim 另设角色 | Hover/pressed 从所在表面推导，叠层放在内容之后、前景之前；不另外发明相互竞争的色块 |
| [UI Set Theme，2026-09-01](https://developers.meta.com/vr/documentation/android-apps/ui-theme/) 与 [LocalContentColors](https://developers.meta.com/vr/reference/metavrx-uiset-compose/1.0.0/metavrx_uiset_compose_theme_localcontentcolors/) | 前景层级跟随最近的容器；修改基础主题可传递到组件；自定义强调色提供对比度约束 | 文字/图标应随容器正确配对，失焦选中与禁用不能混用；同一状态计算规则覆盖整个组件族 |
| [Facebook.com 主题技术，2020-05-08](https://engineering.fb.com/2020/05/08/web/facebook-redesign/) | 主题通过共享变量传递到组件；不同产品可使用不同主题 | 用共享语义 token 实施，避免多个组件继续硬编码近似灰色 |

以上不等同于 Meta 所有产品共用唯一 RGB 表；所查公开资料中，Horizon 的角色定义最完整，但属于其 XR/Android UI Set。Backleaf 借用角色组织与配对方法，Windows 密度和键鼠操作仍遵循既定基线。`design/color-system.json` 的具体色值是本项目提案，不能标作 Meta 内部规范。

本轮推导：普通文件保持开放排版；选择由覆盖图标与名称的单一浅色状态层承载，名称不再另加实心色块，图标不再叠独立灰底；名称字重作为额外识别线索。焦点独立，仅键盘出现；浮层靠层级、边界与影子区分，普通文件没有阴影。以相同位置、尺寸、留白和状态优先级形成重复与节奏。明暗方案、状态组合、对比度和实际窗口一起验收后再实施。
# Independent properties / 2026-11-06

Use a separate information window, following the explicit properties/Get Info entry in [GNOME Files](https://help.gnome.org/gnome-help/nautilus-file-properties-basic.html) and [Finder](https://support.apple.com/en-euro/guide/mac-help/mchlp1774/27/mac/27). Backleaf keeps the full name/path selectable, uses logical bytes with familiar automatic units, and calculates a folder only when its properties are requested.

The scanner uses cached Windows entry metadata where available; [Rust documents that DirEntry metadata does not traverse symlinks](https://doc.rust-lang.org/std/fs/struct.DirEntry.html). Before descending it rechecks directory attributes and skips detected reparse/offline/recall entries, following the meaning of [Windows file attribute flags](https://learn.microsoft.com/windows/win32/fileio/file-attribute-constants). Two workers, a 16-job queue, depth-bounded iterative traversal and 200ms latest-snapshot publication bound resource use. Closing cancels between filesystem calls; an already-blocked OS call cannot be forcibly interrupted. This is an on-demand logical-size scan, not a physical-allocation measurement or transactionally frozen tree; F5 rechecks the selected identity and recomputes.
