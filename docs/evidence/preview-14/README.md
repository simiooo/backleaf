# preview.14 本机验证

81项Release测试、fmt、all-target Clippy和构建日志见本目录。包含真实Shell IDataObject/UTF16往返、不可变身份、同步/异步原生IDropTarget协商及实际IFileOperation移动；目标已移动原文件时PerformedDropEffect为NONE，防止源端重复删除。

自绘扩展菜单/动态子菜单、Esc焦点和选区返回、7-Zip测试单包1文件30字节/多包2文件60字节均有实机结果。快速OLE移动Beta.txt完成1/1项20/20字节；阶段EXE2735261e29f292b280e13bf7703b9abb2f7b1d80b3678dcc8db028be2ebf667d验证侧栏插入Desktop之前且源文件夹保留、菜单超出主窗口边界后仍有独立窗口画面、Esc与正常关闭。不同阶段EXE摘要及各自范围见interactive-builds.json，旧阶段截图不冒充后续相同字节构建。

自动工具拒绝跨源窗口边界的拖动终点，未绕过限制，跨窗/Explorer完整鼠标手势不计作GUI已通过。协议测试也不冒称这类实机手势。混合DPI、NVDA、拔屏与长期压力未实测。staging/CI/下载核对和最终性能记录追加于本目录。

fd5368c 候选在归档菜单焦点冒烟时发现缺陷，CI 41973622289 已取消，无发布。owned-menu-archive-completed.png、owned-path-error-restored.png、owned-menu-rename-completed.png 来自修复后 EXE c2e640901b8fd0bc80081f50d0e6e9b6ca20406bb3b4ec1e0139e1f8ea3ba29c：压缩自动聚焦/失焦完成、内容逐字节一致；路径错误 toast 和草稿焦点恢复；菜单重命名自动聚焦/失焦保存。后续源码仅新增原因注释，最终构建及 staging/CI 摘要另记。

最终源码 ad488a41e6698c6760decb075a69247de44d4cf7，本机 EXE 014913814243666033b5e92df79dfc04cf462fb9ddc9464906e1324a15b3951d。local-build.json 记录输入和检查日志摘要；local-staging.json 记录 2026-11-05T05:47:32Z 解压部署及实际交互范围。staging-archive-completed.png 为最终安装版菜单自动聚焦后失焦完成压缩（1 项、33 B），ZIP 内容逐字节一致；staging-7zip-command-result.png 为通过自绘动态子菜单实际调用注册的 7-Zip 测试命令，1 压缩包/1 文件/33 B、未发现错误。

final-performance 使用上述相同本机 EXE、真实可见 GPUI、1120×720 逻辑尺寸、125% 缩放。10 万项 CPU P95 4.5921ms、回调间隔 P95 12.7091ms、峰值工作集 78.04MiB；1000 万项+8 条流光 CPU P95 5.3457ms、回调间隔 P95 15.4365ms、82.41MiB，完整框选 6,999,826 项、身份验证通过并到达最大偏移。两次缓存上限 100 项/129 标签、单帧回调、隐藏动画请求为零。合成控制器未注入 OS 输入，这些不是实际磁盘千万文件遍历、GPU present 延迟或传输吞吐。superseded-fd5368c-* 仅为已取消候选的历史记录。

[CI41976196744](https://github.com/simiooo/backleaf/actions/runs/41976196744) 全部成功，81 项测试、静态检查和打包启动日志分别保存为 ci-*.log。测试版于 2026-11-05T05:55:34Z [发布](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.14-ad488a41)。download-verification.json 记录下载的四个资产与 CI artifact 字节、GitHub 服务端摘要、包内 EXE/设计/Logo/归档库及源包核对；ZIP SHA256 afec281b4663dcd3f2d7571abe2b22dafca9327963cb3a5f155ca4ca6ea037d9。CI EXE 6b8e5a24a7b739ee55e459f77784769aa5ceb9c234018a16c1bbd4c0c3f88f11 与本机编译分开记录。下载版可见启动、独立路径菜单、编辑命令返回主窗自动焦点、Esc 和正常退出通过，截图 downloaded-path-focus.png；不将本机性能数据冒称为 CI EXE 的测量。
