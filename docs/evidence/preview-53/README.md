# preview.53 · 受保护位置提权重开

源码：`971fc48cff9818bb64da8cb3f0e07fcdd704f60d`，已推送 `staging`。

版本：`0.1.0-preview.53`。

- [发布页](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.53-971fc48c)（预发布，x64 与 ARM64 共 7 项资产），2026-11-09T05:51:47Z 发布。
- [两架构发布 CI](https://github.com/simiooo/backleaf/actions/runs/42568755703) 成功（package / package-arm64）；[推送触发的独立无头 CI](https://github.com/simiooo/backleaf/actions/runs/42568750052) 成功。
- 最终资产于 2026-11-09T14:20+08 下载复核完成。[原始核验记录](verification.json)、[CI 状态](ci-final.json)、[无头 CI](ci-headless.json)。

## 变更

普通权限枚举受保护目录（如 `C:\System Volume Information`、`C:\Windows\System32\config`、其他用户目录）返回 `ERROR_ACCESS_DENIED`（`0x80070005`）时，不再只在底部状态栏留下错误文字：

- 界面显示“需要管理员权限”模态，列出目标路径，提供“以管理员身份重新打开”/“取消”；
- 确认后写入一次性 handoff JSON，通过 `ShellExecuteExW("runas")` 启动同一程序；UAC 由 Windows 弹出，取消或启动失败时删除 handoff、保留提示可重试；
- 管理员进程读取并立即删除 handoff，在相同路径与窗口几何（可用时含选区路径与滚动行）打开新窗口，目录加载即自动重试；
- 原普通窗口保留，显示“已打开管理员窗口”；管理员窗口顶部常驻可关闭提示条，权限持续到该窗口关闭（Windows 提权为每进程令牌，无 TTL），不常驻隐藏提权代理；
- 管理员令牌仍被显式 ACL 拒绝时显示真实错误，不循环弹窗；共享冲突等非权限错误不会误弹提权；
- 检测入口只覆盖目录枚举/刷新被拒；文件操作结果面板的批量失败与“交给系统打开”仍按原错误展示。

## 验证

- 本机 fmt、严格 all-target Clippy、145 native（4 项忽略）、56 headless、Release build 通过。
- 两架构 CI 运行 145 native / 56 headless（干净源报告，56/56 通过）/ 安装器回归 27 项（Windows PowerShell 5.1 与 PowerShell 7 各 27 项），包内 CLI smoke 与发布步骤成功。
- 下载全部 7 项发布资产，逐项与 GitHub 服务端 SHA-256、CI artifact 字节比较，并检查两份 `SHA256SUMS`；两个 ZIP 内 `Backleaf.exe`、`headless.json`、`lib/7z.dll`、`Backleaf-v46.fig`、`Logo.png`、`Backleaf-brand-v3.fig`、ARM64 runtime 与各自构建记录/清单一致，包内 `README.txt` 含 preview.53。
- x64 包在本机重新执行 `--session-status-file` CLI smoke（`locked=false`，exit 0）；本机为 x64，不能执行 ARM64 代码，ARM64 smoke 采用 windows-11-arm hosted runner 记录（`locked=false`）。
- 首轮 CI 失败保留说明：托管 runner 以管理员令牌运行测试进程，两个新增无头用例按普通用户假设失败（其中一个随后走到测试平台未实现的 `open_with_system`）；修复提交 `971fc48c` 将无头基线固定为普通用户令牌后全量重跑通过。未跳过或放宽任何门禁。
- 仍待人工实机验收：真实 UAC 同意/取消交互，以及管理员令牌下实际受保护目录的枚举（CI 不模拟 secure desktop）。

| 文件 | SHA-256 |
| --- | --- |
| x64 ZIP | `75f59b2b4e52ff43f12b2cb60ef2eb22c6b961a1104783626296a5c17ac5c08d` |
| ARM64 ZIP | `d6ce26ff35f1c9a02d17e251ef43ddaf23e495c27615d0395d89810e4b13f8d0` |
| x64 EXE | `1219dabbfddd8c7d9ba709b739cbff479c3b82dce301bb4aeb158a42f4ca2fc7` |
| ARM64 EXE | `c92c6973f5681b73f817c2b6d3b76b34b83b1f7242dd34cec07b0a3ce25c7d11` |
| 设计稿 Backleaf-v46.fig | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |

构建记录：[x64](build.json)、[ARM64](build-arm64.json)。SHA 清单：[x64](SHA256SUMS.txt)、[ARM64](SHA256SUMS-arm64.txt)。干净源无头报告节选：[headless.json](headless.json)。

## 发布后安装器热修

发布后 GitHub Releases 下载直链对部分小资产（`build.json`、`SHA256SUMS*.txt`）间歇返回 504，而 ZIP 与 API 资产端点正常。`582c353` 在安装器中加入 API 资产端点回退，离线安装器回归增加到每 PowerShell 版本 28 项；[Installer CI 42572540036](https://github.com/simiooo/backleaf/actions/runs/42572540036) 与[无头 CI 42572540168](https://github.com/simiooo/backleaf/actions/runs/42572540168) 通过。用公网一行命令把 preview.53 安装到隔离目录时，直链 `build.json` 504 后自动回退 API 成功，安装后的 EXE SHA-256 `1219dabb…` 与发布构建记录一致；应用发布字节未改变。
