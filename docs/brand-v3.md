# Backleaf 圆叶文件夹 · v3

2026-11-08，SIM-1030。金黄色文件夹承载文件管理语义，圆叶表情延续现有 Backleaf 品牌。叶子从文件夹内露出，与前后面板组成一个紧凑轮廓。使用内置 image_gen 生成并精修。

- [透明 PNG](../assets/brand/backleaf-logo-v3.png)，1254×1254，保留生成 RGBA。
- [Windows ICO](../assets/windows/backleaf-v3.ico)，16/20/24/32/40/48/64/128/256px 九档。
- [OpenPencil 检查稿](../design/Backleaf-brand-v3.fig)，overview、sizes-light、sizes-dark 三页。
- [明暗总览](../design/brand-v3/overview.png)、[浅色小尺寸](../design/brand-v3/sizes-light.png)、[深色小尺寸](../design/brand-v3/sizes-dark.png)。
- [完整提示词与生成记录](../assets/brand/generation-v3.md)、[导出摘要](../assets/brand/icon-v3/manifest.json)、[独立冷重开](../design/brand-v3/cold-reopen.json)。

设计稿的尺寸、背景、位置与排版可编辑；图标本体是生成位图。16–24px 主要识别文件夹和绿色叶片，较大尺寸保留表情。已逐页查看明暗预览，九档 ICO 的目录项、偏移、PNG 尺寸与透明四角校验通过。独立 Bun 进程重开三页，changedPixels 均为 0。

用户于 2026-11-08 确认采用此图标，并授权替换应用图标、推送和发布新的测试版。运行 PNG/ICO 同步采用 v3，x64 与 ARM64 测试包内同步携带 v3 品牌稿。实际版本与部署验证见 [preview.52 证据](evidence/preview-52/README.md)。

在 `scripts/design` 目录依次运行：

```powershell
bun export-brand-v3.mjs
bun brand-v3.mjs
bun brand-v3.mjs --verify
```

导出仅全画布等比缩小与 ICO 封装，不对生成图做补绘、抠图或改色。原始初稿与精修成品都保留。
