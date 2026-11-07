# preview.36 · 可视区内图片缩略图

源码：`774560226488107f15c8d38f89dfc7000114df19`（commit `7745602`，2026-11-07）
候选版本：`0.1.0-preview.36`

- Release：[test-v0.1.0-preview.36-77456022](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.36-77456022)，pre-release，非 draft，target 为上述源码。
- Package CI：[42366150175](https://github.com/simiooo/backleaf/actions/runs/42366150175)，7m38s，全部步骤成功：fmt、严格 Clippy、127 项逻辑/文件/原生检查、46 项 GPUI 无头交互、Windows PowerShell 5.1 与 PowerShell 7 installer 回归、打包与 CLI smoke、发布。
- 发布资产与 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.36-77456022-win-x64.zip` | `f53224038d53f62b32b1e367bdcaca52d5cf2bd073b91c6c785cbf3ce40451a8` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `01005f436da9206f349c92a9ac22209a290796aeb9e3c7bcf1ba6a5a208dd7b2` |
| `SHA256SUMS.txt` | `19b355ade6318e9813a2b3e0edf85058ad1b92ca87aaa93308206cc922eddadb` |

- 打包记录（[build.json](build.json)）：EXE SHA-256 `86dbbfbf21ee22b28920f3a680928c05a1434fe58df2965586d36c743c206fbe`，headless 46 项，CLI smoke 通过。
- 本地独立验证：匿名下载发布资产后按 [SHA256SUMS.txt](SHA256SUMS.txt) 校验 ZIP 与设计文件均通过；解压后的 `Backleaf.exe` 以 `--session-status-file` 启动退出码 0，会话输出 `{"locked":false,"error":null}`。未替换用户默认安装目录。

## 本批功能：滑窗内按需图片预览

- **可见区渲染**：主网格继续使用 `uniform_list` 虚拟化，`tile()` 只为当前渲染的行（含前后各 2 行预取）调用缩略图请求；不遍历整个目录，不为离屏文件解码。
- **后台解码**：新增 `src/image_preview.rs`，单后台线程串行解码、缩小到最长边 128px，并应用 EXIF 方向；前台 256 项 LRU，目录 generation 变化或窗口隐藏时清空/暂停并释放 GPU 图片，单文件超过 64MB 或解码失败时回退类型图标。
- **布局**：图片文件先保留类型图标占位，缩略图就绪后替换为 `ObjectFit::Contain` 的居中预览；容器为单元格内宽 × 48px，行高 112 = 上 padding 8 + 预览 48 + 间距 8 + 两行名称 36 + 下 padding 12，因此最大化占满且不溢出。宽图可利用整个内宽，竖图受 48px 高度约束。
- **范围**：png/jpg/jpeg/jpe/jfif/gif/webp/bmp/dib/tif/tiff/ico；HEIC/AVIF/视频不解码，GIF/WebP 显示首帧；搜索结果中的图片同样使用缩略图。
- **测试**：无头 `visible_image_cells_decode_thumbnails_lazily` 验证可见 PNG 解码、文本文件不进入缓存；单元测试覆盖格式白名单与 BGRA 像素输出。headless 46 项、native 127 项。

限制：尚未做真实 GPU 屏幕截图与大批量图片滚动压力实测；首版未解码 HEIC/AVIF/视频，也未做 EXIF 缩略图方向之外的高级色彩管理。视觉大小/清晰度需要你的实际窗口确认。

其它内容沿用 preview.35（任务入口隐藏）、preview.34（高级搜索空查询）、preview.33（面包屑）、preview.32（框选滚动）、preview.31（空查询退出）与 preview.30（索引/搜索）。
