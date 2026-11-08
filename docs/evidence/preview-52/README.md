# preview.52 · 圆叶文件夹应用图标

源码：`77b737897e395d91cd440e3529ed3addb93f12bf`，已推送 `staging`。
版本：`0.1.0-preview.52`。

- [发布页](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.52-77b73789)，x64 于 2026-11-08T13:31:53Z 发布，ARM64 随后补齐。
- [两架构发布 CI](https://github.com/simiooo/backleaf/actions/runs/42484622360)，最终 attempt 2 成功；[独立无头 CI](https://github.com/simiooo/backleaf/actions/runs/42484623265) 成功。
- 最终资产于 2026-11-08T13:50:27Z 下载核验完成。[原始核验记录](verification.json)、[CI 状态](ci-final.json)。

## 变更

用户确认的 v3 黄色文件夹与圆叶表情替换运行 PNG 和 Windows ICO。EXE 的图标组 ID 1 保持不变，九档为 16/20/24/32/40/48/64/128/256px，窗口和安装快捷方式继续读取 EXE 图标。x64、ARM64、本地打包同步携带 `Logo.png`、`Backleaf-brand-v3.fig` 和生成记录。两架构构建记录增加 v3、Logo 和品牌稿摘要，并在解压包冒烟时核验。

## 验证

- 本机 fmt、严格 all-target Clippy、141 native、52 headless、Release build 通过。OpenPencil 三页独立冷重开 0 像素差。
- 两架构 CI 最终完成完整检查、52 项 headless、优化构建、打包和包内 EXE CLI smoke；x64 两种 PowerShell 安装器门禁通过，ARM64 离线真实安装器 smoke 通过。
- 下载全部七项发布资产，逐项与 GitHub 服务端 SHA-256 比较，并检查两份 SHA256SUMS。
- 两个发布 EXE 的图标组 ID 1 均读出全部九档 PNG，逐字节 SHA-256 等于已确认 v3 导出；[x64 图标资源](icon-resources-x64.json)、[ARM64 图标资源](icon-resources-arm64.json)。[读取脚本](verify-icon-resources.ps1) 只按数据文件映射 PE，不执行其代码。
- 两个 ZIP 内 Logo、品牌稿、EXE、7z.dll 和 clean-source headless 报告匹配各自构建记录。Logo 与品牌稿也匹配仓库源文件。
- x64 发布包于 2026-11-08T13:32:45Z 解压部署到隔离 staging，实际相邻运行库检查及 CLI smoke 通过，session 未锁定。ARM64 于 2026-11-08T13:50:26Z 解压到独立目录作静态核验，ARM64 执行与安装验证来自 hosted runner，未在本机 x64 执行 ARM64 代码。

| 文件 | SHA-256 |
| --- | --- |
| x64 ZIP | `378052f48fb0ed1372188d40eae4f6b0a5a72cfdfa17c864d6937038e230758c` |
| ARM64 ZIP | `04adf847de8fb31be873f50da7c2873a458a604a1140ed2ab67e6c64be53d9d6` |
| x64 EXE | `65776e1579458464757fa33d4e396c64b3a5256641b01900400699c571c25689` |
| ARM64 EXE | `dac923c9dc8695878204659c0fc52b70ea0e19ef39631979931625c9935d50f9` |

构建记录：[x64](build.json)、[ARM64](build-arm64.json)。当前 v3 PNG SHA-256 为 `1c55607c2dd25d6d2069609497e84a002e41eda6d36e582883cd031fcdfaa420`；品牌稿为 `1ea20cd98d446f8a38b676d11637bf81de19a0029f093bddd6ecba64214fb3bb`。

## 首轮 ARM64 失败保留

首次 ARM64 的 141 native 通过，52 headless 中 `file_properties_open_separately_keep_geometry_and_preserve_full_long_path` 失败，其余 51 通过；[原报告](arm64-attempt1-headless.json)。ARM 步骤将 `-Output` 设成 `headless.json`，与脚本生成报告的路径相同，报告覆盖原始测试输出，因此未保留该次 panic 栈。

同一源码的完整 ARM64 job 重跑后，全部 52 项及打包、发布、安装 smoke 通过。未改断言、未跳过门禁；此次失败根因仍未确定，不将重跑通过表述为已修复测试稳定性。诊断日志路径问题已记录于 SIM-1030 和 Nuto 排错经验。工程交付待产品验收。
