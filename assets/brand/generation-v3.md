# Backleaf Logo v3 · 圆叶文件夹

2026-11-08，SIM-1030。将现有圆叶表情与 Windows 文件夹意象融合。使用内置 image_gen，以 v2 为参考生成并精修，保留生成原始 RGBA。用户已确认采用 v3，运行资源 backleaf-logo.png 和 ../windows/backleaf.ico 同步更新；应用发布的实际验证另见 docs/evidence/preview-52。

- 初始生成：backleaf-logo-v3-initial.png。
- 候选成品：backleaf-logo-v3.png。
- Windows ICO：../windows/backleaf-v3.ico，16/20/24/32/40/48/64/128/256px 九档。
- OpenPencil：../../design/Backleaf-brand-v3.fig，明暗背景和九档小尺寸检查。PNG 是生成位图；可编辑范围为摆放、尺寸、背景、排版，不冒称叶子/文件夹曲线是矢量。
- 导出只作全画布等比缩小和 PNG-compressed ICO 封装，不做去底、补绘或改色；原始生成文件单独保留。不是 Windows 官方图标，也不包含 Windows 商标图形。

## 初始提示词

Use case: logo-brand.
Asset type: ONE finished Windows desktop app icon for Backleaf / 拾径, a friendly file manager.
Input image: reference for the EXISTING brand character only — a plump sage-green leaf with two round dark-green eyes and a small relaxed smile.
Primary request: combine the existing friendly leaf character with the instantly recognizable visual language of a Windows folder into ONE coherent, beautifully simple icon. A warm golden-yellow file folder with a rounded top-left folder tab and a clear open-folder lip is the main silhouette; a plump rounded sage-green leaf peeks naturally from inside the folder on its upper-right side, with its two eyes and smile visible. The folder cradles the leaf, the two shapes overlap and read as one compact character/object, not a folder with a tiny unrelated sticker. Keep the folder dominant enough to recognize “file manager” at a glance; keep the leaf substantial enough to recognize Backleaf. Use a restrained front view with just a hint of layered depth from the folder's two panels.
Style: meticulously polished, simple vector-like graphic with clean smooth antialiased edges; approachable rounded forms and very subtle broad tonal shading for a modern Windows app. Matte surfaces, low visual noise, no texture. Strong silhouette and large coherent color areas; keep the face readable at 32 px and the folder/leaf silhouette readable at 16 px. Leaf tip soft and rounded. Minimal face, no arms or legs.
Palette: warm golden folder, muted fresh sage-green leaf, deep evergreen eyes and smile. Moderate vividness, clear separation from both light and dark desktops. No heavy outlines.
Composition: square image, isolated single icon optically centered, complete mark occupies about 82% of width and 78% of height, approximately equal generous safe padding on all sides. Real transparent background alpha, clean alpha edges, no baked checkerboard or white/black matte. Any depth belongs inside the object; no ground shadow.
No text, lettering, logo words, app tile, rounded-square backdrop, frame, Windows four-pane logo, badges, accessories, papers with text, sparkles, scenery, presentation board, alternate icons or watermark.

## 精修提示词

Refine this exact Backleaf folder-and-leaf icon for production use. Preserve the complete composition and geometry: yellow open folder with its rounded tab, large friendly sage leaf peeking out, two evergreen dot eyes and curved smile, same placement and proportions. Make only these refinements: simplify the rendering into immaculate smooth vector-like surfaces, a calm golden-yellow folder and muted sage-green leaf, solid opaque interiors with at most one very broad gentle tone change per panel, no grain or mottled lighting. Most importantly regenerate the full silhouette with clean smooth antialiased edges and absolutely ZERO neon yellow/green fringes, stray flecks, isolated pixels, dark/white matte, rough edges, glow, or texture outside the object. Keep the folder lip soft and rounded, the leaf plump, the face readable at small sizes. High-end, quiet, friendly desktop app icon. One complete icon on genuine transparent alpha background, square canvas and equal safe margins. No text, app tile, cast shadow, labels, extra objects, mockup or variants.
