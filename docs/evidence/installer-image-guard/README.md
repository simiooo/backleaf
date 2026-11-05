# SIM-1011：安装升级不再按同名进程误拦截

安装器源码965b2623cf6517160c65054bd5c8a4fc7dfca43c已推送staging。应用继续使用preview23，应用源码2573f9127ab45d73d78658d0a7f798794ecc10ae、CI EXE SHA256 032e59b6966609a60f366ded17137726f2b931cdd22ef92d0f5c59b24a18f6b1均不变；无需重发应用ZIP。

用户遇到同名PID8236存在、路径不可读、零线程/句柄且无窗口时，旧安装器仍直接报“Close Backleaf”。before-failure.txt保存新增复现用例在旧脚本上的同一报错。target-preflight.json证明实际默认安装EXE能取得升级锁，未写入字节；没有据此强行终止该进程，也没有认定其确切退出原因。

安装器现在取得当前安装Backleaf.exe的ReadWrite/FileShare.Delete句柄，不写任何字节。运行映像不能取得写访问，因而仍拒绝升级；持有句柄期间拒绝新读取/旧映像启动，允许目录提升、回滚和旧目录清理，最后总是释放。不再查询全局同名进程的Path，因此其他目录运行的Backleaf和不可读的残留记录都不再单独阻止此安装。权限和其他IO错误仍中止升级并说明原因；安装器不会强杀进程。

本机Windows PowerShell 5.1与PowerShell 7各22项回归通过。占用测试将Windows自带ping.exe复制到隔离管理目录并以Backleaf.exe运行，验证真实PE映像被拒绝替换、字节保持、测试进程未被安装器终止，以及它不阻止另一个安装目录升级。仅测试结束时清理测试自己创建的进程；没有对用户进程发出结束命令。其他用例验证零线程未知Path记录不误拦截、下载期间句柄阻止新读取、失败释放、成功提升、回滚、用户数据、路径/归档校验、运行库、并发安装和环境恢复。

2026-11-05 22:41:11+08已在隔离staging真实匿名下载并安装preview23，运行库及EXE启动诊断通过；22:42:18+08在同一隔离目录重复升级成功，EXE摘要不变。公开固定提交脚本已匿名下载并逐字节匹配Git blob，SHA256 2a140b01ea9063f45c0db0b6e0d348af6afe908ab84b9bd0c68c26e6e04eaf93。实际默认用户目录升级未执行：自动审批以“blocked by policy”拒绝该动作、未给更具体原因；不得将隔离安装记为已替用户更新默认安装。

[安装器CI 42047273366](https://github.com/simiooo/backleaf/actions/runs/42047273366)重试后success：两版PowerShell各22项回归及真实匿名安装、运行库/EXE诊断通过。初次尝试在runtime下载遇GitHub HTTP504，原始失败日志保留；同一提交重试，没有放宽摘要或失败门禁。[无头CI 42047273380](https://github.com/simiooo/backleaf/actions/runs/42047273380)也通过。四份安装器CI报告原始字节保留；CI checkout行尾造成的脚本摘要差异与本地、公开脚本分别记录。此单不宣称新增GUI或启动性能验收。

安装/升级preview23可固定到此修复提交执行：

```powershell
& ([scriptblock]::Create((irm 'https://raw.githubusercontent.com/simiooo/backleaf/965b2623cf6517160c65054bd5c8a4fc7dfca43c/install.ps1'))) -Tag 'test-v0.1.0-preview.23-2573f912'
```
