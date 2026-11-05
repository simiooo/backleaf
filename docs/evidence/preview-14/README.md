# preview.14 本机验证

81项Release测试、fmt、all-target Clippy和构建日志见本目录。包含真实Shell IDataObject/UTF16往返、不可变身份、同步/异步原生IDropTarget协商及实际IFileOperation移动；目标已移动原文件时PerformedDropEffect为NONE，防止源端重复删除。

自绘扩展菜单/动态子菜单、Esc焦点和选区返回、7-Zip测试单包1文件30字节/多包2文件60字节均有实机结果。快速OLE移动Beta.txt完成1/1项20/20字节；阶段EXE2735261e29f292b280e13bf7703b9abb2f7b1d80b3678dcc8db028be2ebf667d验证侧栏插入Desktop之前且源文件夹保留、菜单超出主窗口边界后仍有独立窗口画面、Esc与正常关闭。不同阶段EXE摘要及各自范围见interactive-builds.json，旧阶段截图不冒充后续相同字节构建。

自动工具拒绝跨源窗口边界的拖动终点，未绕过限制，跨窗/Explorer完整鼠标手势不计作GUI已通过。协议测试也不冒称这类实机手势。混合DPI、NVDA、拔屏与长期压力未实测。staging/CI/下载核对和最终性能记录追加于本目录。

fd5368c 候选在归档菜单焦点冒烟时发现缺陷，CI 41973622289 已取消，无发布。owned-menu-archive-completed.png、owned-path-error-restored.png、owned-menu-rename-completed.png 来自修复后 EXE c2e640901b8fd0bc80081f50d0e6e9b6ca20406bb3b4ec1e0139e1f8ea3ba29c：压缩自动聚焦/失焦完成、内容逐字节一致；路径错误 toast 和草稿焦点恢复；菜单重命名自动聚焦/失焦保存。后续源码仅新增原因注释，最终构建及 staging/CI 摘要另记。
