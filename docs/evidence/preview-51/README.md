# preview.51 · Windows ARM64 与 i18n

源码：`9c67faf0eba04886778e944f90894dec863752f1`（commit `9c67faf`）
候选版本：`0.1.0-preview.51`

- Release：[test-v0.1.0-preview.51-9c67faf0](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.51-9c67faf0)
- Package CI：[42470570152](https://github.com/simiooo/backleaf/actions/runs/42470570152)，成功
  - x64 job `package`：8m25s，fmt/严格 Clippy/native 141/headless 52/installer 22/package smoke/publish
  - ARM64 job `package-arm64`：8m46s，native/headless 52/package smoke/ARM64 installer offline smoke/publish
- 测试：native 141、headless 52、installer 22（x64）；ARM64 同套 native/headless；fmt、严格 Clippy 通过。
- 安装：`irm https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1 | iex`

## 资产

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.51-9c67faf0-win-x64.zip` | `bad16b9ac3d8d32a22b79e620f5994999377d93559dadb1795799490c9590531` |
| `Backleaf-0.1.0-preview.51-9c67faf0-win-arm64.zip` | `833e4950ff83fcb41c16170f6f6b4ee195031a1dec86d3649f7b32864e295504` |
| `build.json` | `734cc0ceb9c31552ba8e2123f485033cdb9755ed20e36bf8489611f41dfaf65c` |
| `build-arm64.json` | `dab37d893f5b9bec69f2221e9fdd4b5a68ba9265ff58ed3302828cf97869db56` |
| `SHA256SUMS.txt` | `5d5fa69020f97d20ae9862a5be01d74847444fa9cfbcea267e033a2093a79628` |
| `SHA256SUMS-arm64.txt` | `48f63fcf90ba8b0e1c5e51dc5e141b26803d52d7e58a06edcef765eb857a582d` |

- x64 EXE SHA-256：`87514fd46c73d4dda2bd9c27f1ebf5ebf1325479f59f9f09c342445a9c5e9062`
- ARM64 EXE SHA-256：`57a145ddbdd465d7ea73c5124d0b22426923533adef53355d2292d9c124a6c48`
- `build.json` 与 `build-arm64.json`：x64 未写 `architecture`（保持兼容），ARM64 写 `architecture: arm64`；两者 headless count 52、CLI smoke 通过。

## ARM64

- 原生 `aarch64-pc-windows-msvc` 构建，`CARGO_BUILD_TARGET` + VS ARM64 developer environment。
- `7z.dll` 使用 7-Zip 26.05 ARM64 安装包提取的 ARM64 DLL；`src/operations/archive.rs` 按 target_arch 选择 x64/ARM64 嵌入。
- app-local runtime 从 VS ARM64 redistributable 复制；跳过历史 x64-only `vcruntime140_1.dll`，其余 CRT DLL 全部为 `0xaa64`。
- PE 检查（解压 ARM ZIP）：`Backleaf.exe`、`lib/7z.dll`、所有 CRT DLL 均为 `0xaa64`。
- 安装器按 `PROCESSOR_ARCHITECTURE=ARM64` 选择 `...-win-arm64.zip`、`build-arm64.json`、`SHA256SUMS-arm64.txt`，校验包内 ARM runtime，不再下载 x64 runtime。
- `ARM64 installer offline smoke`：在 ARM64 runner 上用本地构建资产跑真实 `Install-Backleaf`，receipt `architecture=arm64`、安装后 EXE CLI smoke 退出 0。

## i18n

- Explorer 右键菜单标签：`BACKLEAF_LANG` → `%LOCALAPPDATA%\Backleaf\language.txt` → Windows UI 语言；中文“在 Backleaf 中打开”，英文“Open in Backleaf”。
  - 当前机器 `BACKLEAF_LANG=en/zh` 的 installer helper 实测分别得到 `Open in Backleaf` / `在 Backleaf 中打开`；安装后当前中文系统标签为中文。
  - 限制：Windows Explorer 的静态 shell 文案只在安装/升级时写入；切换应用语言后需要重新运行安装器刷新。
- 应用内滚动条无障碍名称改为走 `i18n::tr`（文件滚动条/侧栏滚动条/内容搜索结果滚动条）。
- 预览.50 的菜单/高亮/滚动条改动保留。

## 验证

- Release 资产经 `gh release download` 匿名下载；`SHA256SUMS` 与 ARM64 sums 校验通过。
- x64 打包 EXE `--session-status-file` 退出码 0，`{"locked":false}`。
- ARM64 包 CI smoke 通过；ARM64 installer offline smoke 通过。
- 网络限制：CI 期间 GitHub release 下载多次 504，最终 x64/ARM package 均通过；ARM job 的原联网 installer smoke 被移除，改为离线真实安装器验证，avoiding transient CDN failure。

## 限制

- ARM64 仅在 windows-11-arm hosted runner 构建/测试/smoke，未在实体 ARM 设备长时间试用。
- 未发 x86（32-bit）包；只发 x64 与 ARM64。
- ARM64 安装器离线烟测使用本地资产，未覆盖 GitHub 下载路径（x64 已覆盖）；联网安装依赖 GitHub CDN。
