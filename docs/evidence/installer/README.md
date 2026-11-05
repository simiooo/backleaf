# SIM-998 · PowerShell 一行安装

2026-11-05，用户明确要求公开仓库，GitHub回读为PUBLIC；安装入口为仓库根install.ps1，文档见../../install.md。UI及应用版本仍为preview.15/v38，没有改动GPUI视图。

本机PowerShell 5.1与7均通过16项真实文件系统回归，包括首装、重复安装、升级保留用户数据、运行中阻止升级、下载失败、GitHub摘要错误、缺资产、元数据来源不符、包内EXE不符、替换失败恢复、拒绝非托管目录、互斥锁、ZIP/路径边界和临时清理。网络与进程枚举替换为夹具，替换失败通过注入单次目录移动错误验证，不执行假EXE。

online-5.json / online-7.json分别记录匿名下载已发布preview.15资产、GitHub SHA256/清单和内部EXE/DLL核验、实际解压安装，以及安装后真实EXE的无窗口诊断。应用源786c6e53，EXE SHA256为acf3aef0547f608cb7113e31967255c4fd8bcac4c888092ad4dcd4b5db87b4bc。安装脚本源码由后续CI报告标识，不能将应用源码误写为安装脚本提交。

GitHub installer工作流在两种PowerShell上运行离线回归和匿名发布包安装/CLI冒烟；Windows测试包工作流增加同样的离线安装门禁。最终工作流状态、公共raw入口及默认开始菜单安装结果在完成后补充。
