# preview.55 · 权限重开修复、后台重试边界与卸载脚本

源码：`f893b2ace65555e33d2dc0049bff783a97ae2f9c`，已推送 `staging`。

版本：`0.1.0-preview.55`。

- [发布页](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.55-f893b2ac)（预发布，x64 与 ARM64 共 7 项资产），2026-11-09T08:38:58Z 发布。
- [两架构发布 CI 42586191583](https://github.com/simiooo/backleaf/actions/runs/42586191583) 成功（package / package-arm64）；[推送无头 CI 42586159526](https://github.com/simiooo/backleaf/actions/runs/42586159526) 成功。
- [安装器 CI 42586159457](https://github.com/simiooo/backleaf/actions/runs/42586159457) 成功：安装回归 34/34、卸载回归 6/6（Windows PowerShell 5.1 与 PowerShell 7 各一遍）。
- 最终资产于 2026-11-09T17:01+08 下载复核完成。[原始核验记录](verification.json)、[CI 状态](ci-final.json)、[无头 CI](ci-headless.json)。

## 变更

- **修复管理员重开**：`ShellExecuteExW` 的 exe 路径与参数是未 NUL 结尾的宽字符串，Windows 会把相邻内存当成文件名，报“找不到文件 Backleaf.exe…”。现在 `elevation.rs` 的启动参数统一补 NUL，并加单元回归；权限提示中的“以管理员身份重新打开”可正常提权。
- **关闭后台无限重试**：目录加载/刷新因权限或不可用失败后，停止该目录的 `FindFirstChangeNotification` 监听与自动刷新。此前通知句柄持续失败、周期身份探测每 2 秒把目录标脏，形成无用户操作的刷新循环。现在只有用户确认权限提示、右键“以管理员身份打开”或 F5 才重试一次；成功刷新后自动恢复监听。无头测试新增“拒绝后监听已挂起”断言。
- **右键“以管理员身份打开”**：选中单个文件夹时打开该文件夹的管理员窗口；空白处打开当前位置；已是管理员窗口时不再弹 UAC。菜单项与既有权限提示共用同一 handoff 路径。
- **卸载脚本**：新增 `uninstall.ps1`。默认保留 `%LOCALAPPDATA%\Backleaf`，`-RemoveUserData` 一并删除；先校验安装 receipt 并确认应用已关闭，只删除目标指向本安装 EXE 的开始菜单/桌面/任务栏固定项与 HKCU Explorer 右键项，再删除安装器文件；拒绝非安装器目录、路径重叠、盘根与含 junction/symlink 的路径。6 项离线回归覆盖上述边界。
- 安装器同步支持原生架构检测（x64/ARM64、x64 模拟、32 位 WOW64 与未知回退），见 [install.md](../../install.md)。

## 验证

- 本机 fmt、严格 all-target Clippy、149 native（含新增 NUL 终止单元测试）、56 headless（含“拒绝后挂起监听”断言）、Release build 通过。
- 两架构 CI：native/headless 56/56、安装器 34/34、卸载 6/6、包内 CLI smoke 与发布步骤成功；ARM64 由 windows-11-arm hosted runner 完成构建与 smoke。
- 下载全部 7 项资产，逐项与 GitHub 服务端 SHA-256、CI artifact 字节比较，并检查两份 `SHA256SUMS`；两个 ZIP 内 `Backleaf.exe`、`headless.json`、`lib/7z.dll`、`Backleaf-v46.fig`、`Logo.png`、`Backleaf-brand-v3.fig`、ARM64 runtime 与各自构建记录/清单一致，包内 `README.txt` 含 preview.55。
- x64 包在本机重新执行 `--session-status-file` CLI smoke（`locked=false`，exit 0）；当前机器为 x64，ARM64 采用 hosted runner smoke（`locked=false`）。
- 跳转列表回归仍通过：隔离状态目录下模型 pinned=1、views=1、common=7、drives=2，系统 `CustomDestinations` 被重写并包含四个分组标题与“打开新窗口”任务，已有快捷方式含 AppUserModelID，`--new-window`/`--view` 启动冒烟 exit 0。
- **仍待人工实机验收**：真实 UAC 同意/取消后点击管理员窗口、右键“以管理员身份打开”、任务栏跳转列表点击；CI 不模拟 UAC 交互与任务栏 UI。

| 文件 | SHA-256 |
| --- | --- |
| x64 ZIP | `77911bba04bd00c69a299d3e8e828e6ce08011d0594e80041c3efcff6e6d767c` |
| ARM64 ZIP | `47ca29690af50b5e6295f9c8d98e591bd2f17319fcb1b24456f3a882d5ad7e01` |
| x64 EXE | `1797fd5cdb27614d9c490515e9e9175e491b9b5878c66b1488fb2c5ac042e66a` |
| ARM64 EXE | `9edfebc8dceea11645c9bc92a6a3d964bd32eada3f6c92035429e1ca78bf374e` |
| 设计稿 Backleaf-v46.fig | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |

构建记录：[x64](build.json)、[ARM64](build-arm64.json)。SHA 清单：[x64](SHA256SUMS.txt)、[ARM64](SHA256SUMS-arm64.txt)。干净源无头报告节选：[headless.json](headless.json)。安装器/卸载回归：[installer-tests-5](installer-tests-5.json)、[installer-tests-7](installer-tests-7.json)、[uninstall-tests-5](uninstall-tests-5.json)、[uninstall-tests-7](uninstall-tests-7.json)。
