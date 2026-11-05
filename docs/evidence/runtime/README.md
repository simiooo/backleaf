# SIM-999 · 安装环境与VC++运行库

安装器审计发现preview.15 EXE直接依赖MSVCP140.dll、VCRUNTIME140.dll、VCRUNTIME140_1.dll，原便携ZIP未携带，开发机与CI已有系统运行库使旧CLI冒烟无法发现这一缺口。

修复保持普通用户安装：安装器下载独立的[runtime-v1支持包](https://github.com/simiooo/backleaf/releases/tag/runtime-v1)，将微软原版x64 CRT与声明放在应用目录，不安装机器级redist，不修改系统目录或全局PATH。DLL来自Visual Studio的Microsoft.VC143.CRT redist目录，分发流程逐项核验微软签名、PE架构和摘要。ZIP SHA256为058e48347c973ad4c314cdc3997cb09419b46ab7c07b7e2a4466357716040797；原始来源、版本和条款入口均随包提供。应用EXE仍为786c6e53编译的preview.15，没有伪造新应用版本。

运行库[CI 41988409665](https://github.com/simiooo/backleaf/actions/runs/41988409665)发布成功。最终安装器c6507e07的[CI 41989590242](https://github.com/simiooo/backleaf/actions/runs/41989590242)在Windows PowerShell 5.1与PowerShell 7各通过20项故障回归、真实匿名下载安装、三个VC DLL的明确应用目录加载探针，以及安装后EXE诊断。ci目录是下载的四份实际报告，运行库文件清单/摘要和加载路径均在报告内。

本机2026-11-05T07:46:40Z，将已核验的同一CI应用包与真实公开运行库安装步骤组合部署至独立staging；三个DLL实际加载路径均指向该目录，EXE诊断退出0。此处复用既有已核验应用包，完整网络安装由CI覆盖；没有声称在卸载系统VC运行库的干净虚拟机验证，也没有操作系统目录或继续桌面自动化。staging.json记录准确范围。原本机完整下载遇到慢速超时，已将连接与整体超时分开，整体预算延长为15分钟。

新增环境回归确认PATH未改变、调用会话TLS设置在成功及失败后恢复、LOCALAPPDATA缺失有明确错误。正常Windows账户无需额外配置环境变量、API密钥或开发工具。安装参数和环境边界见[安装说明](../../install.md)。

[微软再分发及app-local部署依据](https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files)。后续应用工具链升级须同步更新经过验证的运行库，app-local依赖的安全更新随Backleaf发布维护；原始preview.15便携ZIP仍保留不变，使用新的安装入口会自动补齐支持包。
