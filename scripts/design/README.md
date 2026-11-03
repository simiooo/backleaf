# 重建设计文件

工具固定为 OpenPencil CLI 0.17.0-beta.1，使用其原生场景图、变量、自动布局、字体测量与 `.fig` IO。Bun 执行包内 TypeScript；没有把截图当作可编辑页面。

在 `scripts/design` 目录执行：

```powershell
npm ci --ignore-scripts --no-audit --no-fund
bun build.mjs
bun verify.mjs
```

`build.mjs` 从 `layout.mjs` 生成 44 页、PNG 和 `.fig`，先做布局审计，通过后发布主稿与生成源。`verify.mjs` 必须在另一个进程运行：独立导入主稿，检查页数与图层数量，再渲染每页与 `design/v6` 比较。证据保存在 `design`，临时重开预览存于被忽略的 `.local`。

重建会替换当前主稿和 v6 预览，先保存正在编辑的主稿副本。SDK 与桌面编辑器的版本可能不同；不要让旧版编辑器的未保存副本覆盖新生成文件。

只检查局部草稿可先设置 BACKLEAF_DESIGN_PAGES 为逗号分隔的页面 key，再运行 bun build.mjs --preview-only；草稿输出在 .local，不发布部分主稿。正式构建必须不设置该环境变量。

`bun verify.mjs --text-only` 可独立核对所有页的图层数量与精确文字，并记录当前主稿 SHA256。完整验证也检查原始文字，视觉比较使用既定容差并保留实际差异数量，不能把“通过”改写成“逐像素完全相同”。
