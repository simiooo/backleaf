# preview.44 · 冷启动优化

源码：`4b688305be3caa5e06529ff42a5c652ac066ce91`（commit `4b68830`，2026-11-07）
候选版本：`0.1.0-preview.44`

- Release：[test-v0.1.0-preview.44-4b688305](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.44-4b688305)，pre-release，非 draft，target 为上述源码。
- Package CI：[42383893756](https://github.com/simiooo/backleaf/actions/runs/42383893756)，7m33s，全部步骤成功：fmt、严格 Clippy、134 项逻辑/文件/原生检查、51 项 GPUI 无头交互、installer 回归、打包与 CLI smoke、发布；发布构建在 headless 门禁后执行 `scripts/native/apply-vendor-patches.ps1`、重新 release 构建并记录 vendor patch。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.44-4b688305-win-x64.zip` | `ced8d31e06fad6449c68630f15abd9a945281aad885daebdbff56274c73617ce` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `5bac07588764226993c5ab6695acf2b4bc67c150e809acec1c2e842c6ede63b1` |
| `SHA256SUMS.txt` | `8c6f8f0d0229b195ad5870bb53771bbfc3c018015751723f907bf51f726e1726` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `ff9732e9b0eee7ebc17c7b7389bc603596ffdd512e1b4c146c4af9f71eca2fbd`，headless 51 项，`vendor_patch=directwrite-font-update-no-check`，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批内容

### 1. 字体缓存更新检查移出冷启动

GPUI 的 Windows 平台初始化原来会调用 `GetSystemFontCollection(..., checkForUpdates=true)`，同步校验/重建系统字体缓存。应用启动只需要现有字体集合；补丁把该检查关闭：

- 补丁文件位置：`vendor/zed/crates/gpui_windows/src/direct_write.rs`
- 应用脚本：`scripts/native/apply-vendor-patches.ps1`（幂等；CI 在 headless 门禁后执行并重建 release；源码构建打包前需手动执行）
- 代价：应用运行期间新安装的系统字体需要重启后才会出现；自定义字体加载不受影响。

### 2. 启动阶段追踪

`BACKLEAF_STARTUP_REPORT=<文件>` 环境变量启用；未设置时仅一次环境变量查询，不写文件。记录阶段：

`main → options → window_state → platform_run → window_open → new_enter → new_workers → new_index → new_ole → new_done → first_ready_render`

`first_ready_render` 在目录快照完成且文件缓存非空的第一帧记录。

## 本机基准（400 文件目录、独立状态目录、各 5 次）

| 指标 | 优化前中位数 | preview.44 中位数 |
| --- | ---: | ---: |
| 首次可交互渲染（`first_ready_render`） | ~403 ms | **~237 ms（-41%）** |
| 进程启动到可交互报告（外部 stopwatch） | ~496 ms | **~305 ms（-39%）** |
| `--no-index` 首次可交互渲染 | ~401 ms | ~255 ms |
| ready 时工作集 | 未记录 | ~65 MB |

原始样本见 [startup-samples.json](startup-samples.json)。

主要剩余成本：GPUI/DirectX 平台与窗口初始化约 200–250ms（字体集合读取仍占约 120–150ms），首帧目录快照+布局约 20–40ms。

## 验证

- 全部 51 项 headless、134 项 native、fmt、严格 Clippy、release 构建通过。
- 本机测量为 release 包、warm OS 文件缓存条件下的重复启动；不是重启系统后的绝对冷启动，但优化前后的测量条件一致。

限制：未做 GPU 时间线级 profile；未优化 DirectX 设备创建；发布包依赖 apply-vendor-patches 补丁，源码直接 `cargo build` 不会自动包含。

其它内容沿用 preview.43（内容搜索浮层/查询优化）、preview.42（中英界面）、preview.41（内容搜索）、preview.40（Shell 菜单空状态）、preview.39（容量单位）、preview.38（焦点不重解码）、preview.37（预览溢出）、preview.36（图片缩略图）、preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
