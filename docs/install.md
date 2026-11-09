# PowerShell 安装

仓库和发布包已公开。Windows PowerShell 5.1 和 PowerShell 7 可直接执行：

```powershell
irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex
```

默认优先安装最新已发布正式版（如 `v0.1.0`），没有正式版时回退最新预览版。在当前用户开始菜单创建 Backleaf 入口；不要求管理员权限，不改变执行策略，也不强制启动应用。升级前关闭 Backleaf，再运行相同命令。文件任务进行中不会被安装器强制中断。

安装位置：`%LOCALAPPDATA%\Programs\Backleaf\current`。用户数据位置：`%LOCALAPPDATA%\Backleaf\operations`。安装与升级不删除用户数据。卸载时关闭应用，删除安装目录和开始菜单的 Backleaf 快捷方式即可；用户数据可保留。

升级保护直接打开并锁定当前安装的Backleaf.exe：取得写访问但不写入任何字节，拒绝新的读取/旧程序启动，同时允许安装器进行目录重命名和失败回滚；成功、失败后都释放锁。运行中的可执行映像无法取得该锁，因此仍会要求先关闭应用。其他目录的同名进程，以及退出后残留且路径不可读的进程记录，不再单独阻止升级；安装器不会强制结束任何进程。权限或其他文件错误会显示无法取得升级锁及具体原因。

安装器同时下载已发布的runtime-v2支持包，将微软原版x64 VC++运行库放在应用目录；不要求用户另装运行库，不写系统目录、不注册机器级组件，也不要求Rust、Visual Studio或GitHub CLI。CI核验微软签名与架构，安装时核验发布资产与每个DLL摘要，通知和来源清单放在licenses/MSVC-runtime。支持包独立于应用ZIP发布，应用编译版本不变；后续工具链升级时须同步更新经过验证的运行库版本。依据：[微软app-local部署](https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files)。

环境配置使用当前Windows账户已有的LOCALAPPDATA；缺失时明确报错，不把程序装到猜测的目录。USERPROFILE及TEMP/TMP由应用读取Windows现有值，安装器不改写。无需API密钥或手动设置环境变量。安装器不修改用户/系统PATH，因此终端中暂不注册backleaf命令；从开始菜单或安装目录启动。安装过程临时启用TLS1.2并在成功/失败后恢复调用会话原设置，不修改全局执行策略。下载采用现有网络代理配置，不替用户创建代理或凭据。

指定发布版本、位置、桌面入口或安装后启动：

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1))) -Tag v0.1.0 -InstallDir 'D:\Apps\Backleaf' -DesktopShortcut -Launch
```

`-NoShortcut` 跳过所有快捷方式。`-Tag` 选择精确发布 tag，也可用于回退版本或指定预览版（如 `test-v0.1.0-preview.52-77b73789`）；省略时优先按发布时间选择正式版。正式标签须指向完整、不可变的 40 位源码提交，并与构建记录逐字匹配。此入口从 staging 获取安装脚本，实际应用来自 GitHub 已发布资产，不从源码编译。网络不可用或 GitHub API 超限时，修复网络或等待额度恢复后重试，不会改动已安装应用。

下载先核验 GitHub Releases API 的 SHA256 digest 和大小，再核对 SHA256SUMS、CI 版本/源码记录及包内 EXE、7z.dll 哈希。GitHub 直链对单个资产返回 5xx 时，安装器自动改用 API 资产端点重试同一字节。解压前拒绝目录逃逸、绝对路径、NTFS流、重复/设备名称和链接，限制条目数、下载及展开大小。校验全部通过后在同卷替换应用目录；替换失败恢复原版本。非本安装器管理的 current 目录不会覆盖，路径中的 junction/symlink 会被拒绝。哈希证明下载与发布内容一致。

CI 在 Windows PowerShell 5.1 和 PowerShell 7 分别执行28项真实文件系统离线故障回归（含正式版优先、显式预览标签、正式版安装、完整源码提交约束与 GitHub 直链失败回退 API 端点），以及匿名下载已发布包、安装、运行 EXE 无窗口诊断和哈希核验。离线测试替换网络和一次目录提升故障；用隔离副本运行真正的Windows可执行映像验证占用拒绝与同名其他目录进程不误拦截，并验证下载期间拒绝新读取、成功/失败释放锁和回滚。零线程未知路径记录保留为误拦截复现输入，生产安装器不再枚举进程名。没有将这些检查称为 GUI 验收。

API 选择与校验依据：[GitHub Releases REST API](https://docs.github.com/en/rest/releases/releases)。`/latest`不包含 prerelease，因此安装器查询已发布列表。
