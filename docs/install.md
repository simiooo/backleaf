# PowerShell 安装

仓库及测试包已公开。Windows PowerShell 5.1 和 PowerShell 7 可直接执行：

```powershell
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex
```

安装最新已发布测试包（包含 prerelease），在当前用户开始菜单创建 Backleaf 入口；不要求管理员权限，不改变执行策略，也不强制启动应用。升级前关闭 Backleaf，再运行相同命令。文件任务进行中不会被安装器强制中断。

安装位置：`%LOCALAPPDATA%\Programs\Backleaf\current`。用户数据位置：`%LOCALAPPDATA%\Backleaf\operations`。安装与升级不删除用户数据。卸载时关闭应用，删除安装目录和开始菜单的 Backleaf 快捷方式即可；用户数据可保留。

指定发布版本、位置、桌面入口或安装后启动：

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1))) -Tag test-v0.1.0-preview.15-786c6e53 -InstallDir 'D:\Apps\Backleaf' -DesktopShortcut -Launch
```

`-NoShortcut`跳过所有快捷方式。`-Tag`选择精确发布 tag，也可用于回退版本；省略时按发布时间选择最新 Backleaf 测试包。此入口从 staging 获取安装脚本，实际应用来自 GitHub 已发布资产，不从源码编译。Windows 下载脚本时若网络不可用或 GitHub API 超限，修复网络或等待额度恢复后重试即可；不会改动已安装应用。

下载先核验 GitHub Releases API 的 SHA256 digest 和大小，再核对 SHA256SUMS、CI 版本/源码记录及包内 EXE、7z.dll 哈希。解压前拒绝目录逃逸、绝对路径、NTFS流、重复/设备名称和链接，限制条目数、下载及展开大小。校验全部通过后在同卷替换应用目录；替换失败恢复原版本。非本安装器管理的 current 目录不会覆盖，路径中的 junction/symlink 会被拒绝。哈希证明下载与发布内容一致。

CI 在 Windows PowerShell 5.1 和 PowerShell 7 分别执行真实文件系统的离线故障回归，以及匿名下载已发布包、安装、运行 EXE 无窗口诊断和哈希核验。网络/进程/一次重命名故障在离线测试中替换，其余 ZIP、摘要、锁、目录替换与回滚均为真实操作。没有将这些检查称为 GUI 验收。

API 选择与校验依据：[GitHub Releases REST API](https://docs.github.com/en/rest/releases/releases)。`/latest`不包含 prerelease，因此安装器查询已发布列表。
