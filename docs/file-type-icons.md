# 文件类型图标与 EXE 内嵌图标

SIM-963，2026-11-04。先在 OpenPencil 完成 v14 矢量探索，再按产品反馈使用内置 `image_gen` 生成 15 张透明 PNG，并以 v15 明暗网格、小尺寸、选择/剪切、EXE 读取前/成功/回退完成设计复核。v15图标稿为112页；当前交付master为`design/Backleaf-v18.fig`，129页，保留全部图标探索与后续窗口栏状态。容器、文字和交互状态可编辑；生图图标是原始位图，不宣称为可编辑矢量。

原图及完整提示词在 `assets/file-types-generated/source`、`generation.json`。OpenPencil 按可见 alpha 边界裁去透明留白，并等比归一到 44 px 槽位内约 40 px 的实际图形，再导出 192 px 运行资源；保留原图与图案、透明度。15 张运行 PNG 合计 402,616 字节，固定图像解码尺寸；运行程序不读取大原图、不下载图标。原生普通类型通过稳定资产路径共享 GPUI 图像缓存，不按文件名生成新纹理。

覆盖 Office 三件套、PDF、应用、安装包、图片、视频、音乐、归档、电子书、字体、源码及未知文件。源码、标记语言与配置类采用统一代码符号，具体格式由文件扩展名表达。保留 47 个语义分类以便后续增加视觉变体。图标是文件名展示，不表示已验证文件内容、格式有效或运行安全。

| 规则 | 结果 |
| --- | --- |
| 目录，包括 `资料.png`、`应用.exe` 等目录名 | 始终显示文件夹 |
| DOC/DOCX/DOCM/ODT、XLS/XLSX/XLSM/CSV、PPT/PPTX/PPSX/ODP | 分别显示文字、表格、演示图标 |
| EXE/COM/SCR | 优先后台读取文件内嵌图标，失败使用默认应用图标 |
| MSI/MSIX/APPX 及包/bundle，DMG/PKG/APK/DEB/RPM 等 | 安装包图标 |
| PNG/JPEG/HEIC/AVIF/SVG/RAW 等；MP4/MKV/MOV/WEBM 等；MP3/FLAC/WAV/OPUS 等 | 对应图片、视频、音乐图标 |
| ZIP/7Z/RAR/TAR/GZ/XZ/ZST 等；EPUB/MOBI/AZW3/FB2/DJVU/CBZ 等；TTF/OTF/TTC/WOFF2 等 | 对应归档、电子书、字体图标 |
| JS/TS/JSX/TSX/PY/RS/GO/C/CPP/CS/JAVA/SWIFT/KT/DART/PHP/RB 等 | 代码图标 |
| 扩展名大小写、最后一个后缀 | ASCII 不区分大小写，`report.pdf.exe` 按应用处理 |
| `.ts`、`.mts` | 默认按 TypeScript；不读取内容猜测传输流 |
| 无扩展名、未知扩展名、非 ASCII 仿冒后缀 | 通用文档；Dockerfile、Makefile 等明确名称另有分类 |

扩展名清单在 `scripts/design/file-icons-spec.mjs`。Office 清单参考 [Microsoft 文件格式表](https://learn.microsoft.com/office/compatibility/office-file-format-reference)，电子书清单参考 [calibre 文档](https://manual.calibre-ebook.com/faq.html)，Windows 包类型参考 [MSIX 包格式](https://learn.microsoft.com/en-us/windows/msix/package/app-package-formats)。同名格式冲突是产品展示规则，不是来源对内容类型的保证。

EXE 通过 [SHDefExtractIconW](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shdefextracticonw) 只提取资源，不启动程序，不调用扩展名关联程序或第三方 Shell 图标处理器。一个后台线程、32 个待处理请求、32 个结果、128 个 LRU 正/负缓存项；只由可见文件格与搜索结果请求。导航/刷新代次变更会清空待处理项、拒绝旧结果并释放 GPU 图像。缓存淘汰通过 GPUI `drop_image` 同时释放图集项。不可见/锁屏时暂停接新工作。失败保留默认图标，不弹出逐项错误。

HICON、内存 DC、DIB 都有作用域释放；黑白双背景重建 alpha，兼容传统 AND mask 与现代透明图标。读取前后复核文件长度与修改时间，变化时回退；文件写操作仍使用原有身份校验，图标缓存不参与操作目标的判断。路径读取与提取只在后台进行，慢资源读取不会在关闭窗口时被 UI 线程 join。

验证范围和交付入口见 `docs/evidence/file-icons/README.md`；不把托管 CI 冒烟等同于真实桌面视觉验收。

产品指出首版图案偏小：44 px 槽位中的纸张仅约 28 px 高。v15 改稿依据可见 alpha 边界调整显示，不增大文件格或挤压名称；纸张约 32×40 px，文件夹约 39×31 px，原生 EXE 图标也按相同比例上限归一，保留各自宽高比。`optical-sizing.json` 记录每张原图边界、运行图哈希和显示尺寸。

2026-11-04 追加改稿：文件夹使用新生成的圆润、低描边图案，并同步到左侧 18 px 导航。透明输出在导出阶段保留主轮廓、清理淡白/青色游离像素和微小 alpha 缺口，重新生成平滑抗锯齿；原图及代码图标的修边候选均保留，不覆盖原始生成历史。

v16起在共享位图上叠加缓存的细纸张外轮廓，避免原图大尺寸细边下采样到40px后消失；24px使用独立线宽，不增加白色底板。文件夹和EXE原生资源不叠纸张轮廓。候选cdbf640e已在明暗真实窗口检查，性能证据见chrome-feedback。
