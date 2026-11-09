# preview.54 · 任务栏跳转列表

源码：`8f05397129aabf361930f0b5cdd6445951793405`，已推送 `staging`。

版本：`0.1.0-preview.54`。

- [发布页](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.54-8f053971)（预发布，x64 与 ARM64 共 7 项资产），2026-11-09T07:39:29Z 发布。
- [两架构发布 CI](https://github.com/simiooo/backleaf/actions/runs/42579633503) 成功（package / package-arm64）；[推送触发的独立无头 CI](https://github.com/simiooo/backleaf/actions/runs/42579608872) 成功。
- 最终资产于 2026-11-09T15:52+08 下载复核完成。[原始核验记录](verification.json)、[CI 状态](ci-final.json)、[无头 CI](ci-headless.json)。

## 变更

Windows 任务栏按钮右键新增跳转列表，布局参考资源管理器但**不记录最近访问**：

- 固定：侧栏中用户固定的文件夹入口（按侧栏顺序，最多 10）；
- 视图：侧栏保存的视图入口（最多 10）；
- 常用：当前用户家目录与桌面、下载、文档、图片、音乐、视频（固定列表，最多 8）；
- 分区：当前挂载的逻辑磁盘（最多 8）；
- 任务：打开新窗口（主文件夹）。

每个条目通过 `Backleaf.exe --path "<文件夹>"` / `--view <id>` / `--new-window` 启动独立新窗口，不打开资源管理器。进程、跳转列表和快捷方式使用固定 AppUserModelID `Simiooo.Backleaf`；启动时把已有开始菜单/任务栏固定快捷方式就地补上该 ID，使已固定按钮与运行进程合并且出现列表。侧栏 revision、磁盘快照或语言变化时后台合并重建；用户从系统菜单移除的条目按启动参数过滤。新增 `docs/jump-list.md` 说明数据来源、刷新与边界。

## 验证

- 本机 fmt、严格 all-target Clippy、148 native（4 项忽略）、56 headless、Release build 通过。
- 两架构 CI：145+3 native / 56 headless（干净源报告 56/56）/ 安装器回归 27 项（Windows PowerShell 5.1 与 PowerShell 7 各 27 项）、包内 CLI smoke 与发布步骤成功。
- 下载全部 7 项资产，逐项与 GitHub 服务端 SHA-256、CI artifact 字节比较，并检查两份 `SHA256SUMS`；两个 ZIP 内 `Backleaf.exe`、`headless.json`、`lib/7z.dll`、`Backleaf-v46.fig`、`Logo.png`、`Backleaf-brand-v3.fig`、ARM64 runtime 与各自构建记录/清单一致，包内 `README.txt` 含 preview.54。
- x64 包在本机重新执行 `--session-status-file` CLI smoke（`locked=false`，exit 0）；本机为 x64，ARM64 采用 hosted runner smoke（`locked=false`）。
- 跳转列表实测（下载的 x64 包 + 隔离状态目录）：写入模型 pinned=1、views=1、common=7、drives=2；系统 `CustomDestinations` 文件被重写且包含「固定 / 视图 / 常用 / 分区 / 打开新窗口」标题与 `--path`/`--new-window` 参数；两个已存在的快捷方式均含 `Simiooo.Backleaf` AppUserModelID；`--new-window` 与 `--view` 启动冒烟均 exit 0。诊断模型见 [jump-list-model.json](jump-list-model.json)。
- 仍待人工实机验收：鼠标右键任务栏按钮并逐项点击跳转列表；真实 UAC 同意/取消与管理员令牌下实际受保护目录枚举（CI 不模拟 secure desktop 与任务栏 UI）。

| 文件 | SHA-256 |
| --- | --- |
| x64 ZIP | `395a43b5fb2659086dcf7e9cab374a847fb6d19b73d635b80b58ae72a17e9a77` |
| ARM64 ZIP | `4a06ff9f8339f39d0d9e5b60122982f8f75e8030f0aebaa4280287e959325f15` |
| x64 EXE | `d848bfa821129b2acb21e3660d3d73968d925ad81b090a0094b8103205c96f49` |
| ARM64 EXE | `2397f94947afacf2cef9a84abd0f40d009f0e6798843d02a49ac39726b4ae8a2` |
| 设计稿 Backleaf-v46.fig | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |

构建记录：[x64](build.json)、[ARM64](build-arm64.json)。SHA 清单：[x64](SHA256SUMS.txt)、[ARM64](SHA256SUMS-arm64.txt)。干净源无头报告节选：[headless.json](headless.json)。
