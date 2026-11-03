# 重建设计文件

工具固定为 OpenPencil CLI 0.17.0-beta.1，使用其原生场景图、变量、自动布局、字体测量与 `.fig` IO。Bun 执行包内 TypeScript；没有把截图当作可编辑页面。

在 `scripts/design` 目录执行：

```powershell
npm ci --ignore-scripts --no-audit --no-fund
bun build.mjs
bun verify.mjs
```

`build.mjs` 从 `layout.mjs` 生成 30 页、PNG 和 `.fig`，先做布局审计，通过后发布主稿与生成源。`verify.mjs` 必须在另一个进程运行：独立导入主稿，检查页数与图层数量，再渲染每页与 `design/v4` 比较。证据保存在 `design`，临时重开预览存于被忽略的 `.local`。

重建会替换当前主稿和 v4 预览，先保存正在编辑的主稿副本。SDK 与桌面编辑器的版本可能不同；不要让旧版编辑器的未保存副本覆盖新生成文件。
