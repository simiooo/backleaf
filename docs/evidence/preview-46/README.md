# preview.46 · GPU 并发初始化实验

源码：`84292ce50fed0284af43cfa2875c780cefbc5cec`（commit `84292ce`，2026-11-08）
候选版本：`0.1.0-preview.46`

- Release：[test-v0.1.0-preview.46-84292ce5](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.46-84292ce5)，pre-release，非 draft。
- Package CI：[42440139793](https://github.com/simiooo/backleaf/actions/runs/42440139793)，6m34s，fmt、严格 Clippy、135 项 native、51 项 headless、installer 回归、打包 smoke、发布全部成功；发布构建应用 `vendor/patches/backleaf-startup.patch` 后重建，`vendor_patch=font-update-no-check+gpu-init-async`。
- 资产 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.46-84292ce5-win-x64.zip` | `fe46b497c4d00ff6cc25c319eea68e383330bacd02b579ed69ca1bbcf142b997` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `9d02d04c28df4ed6ac1e253416223e044c8f9b031aa0993bb5adf5c53510194d` |
| `SHA256SUMS.txt` | `b219c8441c3b9b68ce57db90c3c057f0af6b2360ee7c0e51fd04adab0c44dd47` |

- 打包记录（[build.json](build.json)）：EXE `0eb2d749…`，headless 51 项，`vendor_patch=font-update-no-check+gpu-init-async`，CLI smoke 通过。

## 改动

1. **GPU 设备并发初始化**
   - `WindowsPlatform::new` 在完成 OLE 初始化后立即启动 `backleaf-gpu-init` 线程创建 `DirectXDevices`；主线程继续创建字体系统、平台隐藏窗口和消息循环。
   - `attach_gpu` 主线程从 channel 取回设备（`recv`），若线程创建失败则回退同步创建；随后才创建窗口渲染器。
   - 平台状态新增 `gpu_init: RefCell<Option<Receiver<Result<DirectXDevices>>>>`；设备只初始化一次。
2. **统一 vendor 补丁**
   - 之前的字体缓存检查补丁 + 本次 GPU 并发合并为 `vendor/patches/backleaf-startup.patch`；
   - `scripts/native/apply-vendor-patches.ps1` 幂等应用并校验标记；CI 在 headless 门禁后应用并重建 release；源码构建打包前需手动执行。
3. **启动标记**
   - 新增 `platform_created`，可区分 `WindowsPlatform::new` 与 `run` 中 GPU attach/窗口创建的耗时。

## 结果（未达成 190ms 目标）

同一 release binary、400 文件目录、独立状态目录；串行/并发通过临时环境开关各 5 次：

| 阶段 | 串行 GPU | 并发 GPU |
| --- | ---: | ---: |
| `platform_created` 中位数 | ~34ms | ~30ms |
| `platform_run`（含 GPU attach） | ~152ms | ~116ms |
| `window_open→new_enter` | ~215ms | ~195ms |
| `first_ready_render` | ~250ms | ~230ms |

后续 6 次并发样本中位数 `first_ready_render` 约 247ms（empty）/254ms（真实索引），本机系统负载导致波动；原始样本见 [startup-samples.json](startup-samples.json)。

目标 190ms **没有达到**。剩余关键路径：

- 窗口创建 + DirectX 渲染资源（swapchain/pipelines/Composition）：约 80ms
- 首帧目录快照 + 布局/字体 shaping：约 25–40ms
- 进程加载/外部启动开销：约 70ms（不在 `first_ready_render` 内）

这些阶段彼此串行，单纯并发字体/GPU 设备无法继续压缩。

## 真机 GPU 验证

- 解压发布包中的 `Backleaf.exe`，用临时目录启动并置顶截图；文件列表文字、20 个文件卡片、侧栏 `qq651`/桌面/下载等、容量条和文件表面均正常渲染。
- 截图 [real-gpu-window.png](real-gpu-window.png)，SHA-256 `d1096c145826cc461755b3e8b9d5df66cc55bfef7b72dc5a37246bb63ee6b20a`。
- 打包 EXE `--session-status-file` 快速验证退出码 0，输出 `{"locked":false,"error":null}`。

## 风险与限制

- 并发 D3D11 设备创建在旧驱动/远程桌面/多 GPU 环境下可能出现设备丢失或初始化顺序问题；当前只在单机真实 GPU 与 CI 打包 smoke 验证，未覆盖多机矩阵。
- 190ms 目标未达成，不宣称达到；GPU 并发保留为实验性收益（约 20ms 中位改善）。
- 回退方式：`git -C vendor/zed checkout -- README.md crates/gpui_windows/src/direct_write.rs crates/gpui_windows/src/platform.rs`，不应用 patch 即回到 preview.45 行为。

其它内容沿用 preview.45（侧栏家目录名）、preview.44（字体缓存/启动追踪）、preview.43（内容搜索浮层/查询优化）、preview.42（中英界面）及更早 preview。
