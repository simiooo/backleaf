# SIM-998 · PowerShell 一行安装

2026-11-05，用户明确要求公开仓库，GitHub回读为PUBLIC；安装入口为仓库根install.ps1，文档见../../install.md。UI及应用版本仍为preview.15/v38，没有改动GPUI视图。

本机PowerShell 5.1与7均通过16项真实文件系统回归，包括首装、重复安装、升级保留用户数据、运行中阻止升级、下载失败、GitHub摘要错误、缺资产、元数据来源不符、包内EXE不符、替换失败恢复、拒绝非托管目录、互斥锁、ZIP/路径边界和临时清理。网络与进程枚举替换为夹具，替换失败通过注入单次目录移动错误验证，不执行假EXE。

online-5.json / online-7.json分别记录匿名下载已发布preview.15资产、GitHub SHA256/清单和内部EXE/DLL核验、实际解压安装，以及安装后真实EXE的无窗口诊断。应用源786c6e53，EXE SHA256为acf3aef0547f608cb7113e31967255c4fd8bcac4c888092ad4dcd4b5db87b4bc。安装脚本源码由后续CI报告标识，不能将应用源码误写为安装脚本提交。

最终脚本源码adcf5f8834d4421d775cd478001075b5811e801d。[安装CI 41985734534](https://github.com/simiooo/backleaf/actions/runs/41985734534)完成success：两种PowerShell各16项离线故障回归及真实匿名下载安装/CLI冒烟通过。ci目录是实际下载的四份报告，各自记录installer_sha256；已逐一核对其字节与最终源码的Windows CRLF检出一致。ci-artifacts.json记录真实上传的installer-evidence摘要。Windows测试包工作流也增加两种PowerShell的离线安装门禁。

[GPUI无头CI 41985655842](https://github.com/simiooo/backleaf/actions/runs/41985655842)也最终完成success；headless.json是实际下载的16项交互报告，源码2c0f9805。后续adcf5f8仅修正安装器选择curl的路径，不改应用或GPUI测试源码。source-verification.json记录四份安装报告的摘要与最终安装脚本字节核对结果。

default-install.json记录README公开命令在本机执行后的默认安装，时间2026-11-05T07:14:42Z（15:14:42+08），开始菜单快捷方式确实存在、目标指向current/Backleaf.exe，EXE哈希匹配发布内容并通过无窗口诊断。应用未自动启动GUI，不重复无头已覆盖的交互场景。原生窗口尚未覆盖的边界仍以preview.15记录为准。

下载在当前网络出现慢速，使用系统curl（存在时）提供连接/总时限、低速中断及一次重试；无curl时使用PowerShell传输。无需GitHub CLI或账户。源码选择首个curl.exe，CI确实有系统与Git两份curl，已验证此环境正常执行。安装脚本和应用版本来源分别记录，不冒称重新编译或重发了应用。
