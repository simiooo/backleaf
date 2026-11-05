# SIM-1008 · 第三方具体命令验收

沿用已交付的OpenPencil独立菜单基线；本单验证既有交互和命令调用，没有修改菜单实现或另行替换为系统菜单。源码78d86152a6d4dec08c8d388ed9920ba8e79b8163，preview.22。Windows桌面起止未锁定，所有鼠标/键盘和窗口截图观察均通过Computer Use，使用自有隔离文件。

本机真实注册的7-Zip 26.05，CLSID {23170F69-40C1-278A-1000-000100020000}，DLL及输入摘要见provider-and-input.json。没有把菜单可见、二进制协议测试或模拟provider当作具体命令成功。

| 实际路径 | 可核对结果 |
| --- | --- |
| 两文件全选，Shift+F10 → 应用操作 → 7-Zip → 添加到shell-command-fixture.zip | 生成475字节ZIP；逐项核对report α.txt/说明.txt、5160/29字节及原始SHA-256；7-Zip原生完整性检查通过。archive-proof.json、archive.zip、native-archive-test.txt。 |
| 单文件 → 应用操作 → 7-Zip → CRC SHA → SHA-256 | 出现实际“校验信息”窗口，显示5160字节及与源一致的SHA-256。视觉记录checksum-visual.json；此第三方自有对话框未向窗口工具暴露可独立绑定的UIA窗口，未声称机器提取全文。 |
| 单文件 → 同一动态CRC子菜单 → SHA-256 -> report α.txt.sha256 | 实际生成UTF-8摘要文件；独立重新计算源文件摘要，与文件中的64个十六进制字符及Unicode文件名精确一致。checksum-proof.json、checksum.sha256。 |
| 公开CI安装EXE，真实鼠标右键 → 相同独立菜单 → 生成SHA-256文件 | 对确切CI EXE再次实际执行并核对生成文件，ci-command-proof.json、ci-checksum.sha256、ci-menu-tree.txt。 |

本地测试EXE为b47fb14d1e5822a16dbb3a626defc28223922e02fe210f03f0b10e8dd4064b0b；公开CI安装EXE为cd4ee1056f6d6ab64802a2e71b344b2e34fa702a8a97878d1930aa285bf0c8a7，二者均有精确进程、源码与时间记录。最终CI命令核对于2026-11-05 20:52:40+08完成。四层菜单由Backleaf绘制，第三方仅提供项目与命令；校验窗口属于7-Zip本身。

ZIP中的这两种可用本地代码页表示的名称由7-Zip按CP936写入，UTF-8标志未置位。默认UTF-8解释的.NET读取首次核对失败；随后按ZIP标志与实际Windows ACP解码，再精确比较名称和解压字节，且以7-Zip本身作完整性检查。不是忽略名称错误或改变原文件。原始ZIP和检查方式均保留。

menu-geometry.json及menu-stability.json记录应用操作、7-Zip和动态CRC子菜单展开前后；既有主窗口和祖先菜单的坐标、宽高保持相同。它是稳态矩形对比，不是高速录制的闪烁/动画帧证明。首次UIA点击嵌套菜单项被工具以缓存索引不可用拒绝，随后重新观察并使用同一次截图坐标；没有将失败的索引点击计作命令通过。

Backleaf本地与CI验收窗口均已正常关闭。只读校验结果进程没有可独立绑定的窗口，最终按确切PID、启动时间、父进程、EXE及完整命令行核验所有权后，仅清理本次测试创建的该进程，记录见hash-process-cleanup.json；未使用其他UI自动化方式代替Computer Use。界面操作、后台搜索计时和进程清理分别记录。

兼容性范围限定于本机7-Zip 26.05、表中实际执行的命令及单/多文件、希腊字母/中文名称，不泛化为所有第三方扩展；未执行云上传、邮件、打印或其他有外部副作用的注册项。原始输入与生成文件均位于.local的隔离夹具中。
