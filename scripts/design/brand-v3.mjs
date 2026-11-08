import {readFile, writeFile, mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {SceneGraph} from '@open-pencil/scene-graph';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS, IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes, initCanvasKit, createCanvasKitRasterCodec, comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText, editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const root = resolve(import.meta.dir, '../..');
const out = resolve(root, 'design/brand-v3'), file = resolve(root, 'design/Backleaf-brand-v3.fig');
const io = new IORegistry(BUILTIN_IO_FORMATS);
await mkdir(out, {recursive: true});
if (process.argv.includes('--verify')) {
  const bytes = new Uint8Array(await readFile(file));
  const {graph} = await io.readDocument({name: file, data: bytes});
  populateAllFigPages(graph);
  await measureDocumentText(graph);
  const codec = createCanvasKitRasterCodec(await initCanvasKit()), results = [];
  if (graph.getPages().length !== 3) throw Error('Expected three brand pages');
  for (const page of graph.getPages()) {
    const node = graph.getChildren(page.id)[0];
    const png = await headlessRenderNodes(graph, page.id, [node.id], {format: 'PNG', scale: 1});
    const difference = comparePNGs(codec, new Uint8Array(await readFile(resolve(out, page.name + '.png'))), png, {threshold: .05});
    if (!difference || difference.changedPixels !== 0) throw Error('Cold reopen changed ' + page.name);
    results.push({page: page.name, nodes: graph.countDescendants(node.id) + 1, changedPixels: difference.changedPixels});
  }
  await writeFile(resolve(out, 'cold-reopen.json'), JSON.stringify({masterSha256: createHash('sha256').update(bytes).digest('hex'), pages: 3, results, result: 'passed'}, null, 2));
  console.log('Brand v3 cold reopen passed: three pages, zero changed pixels');
  process.exit(0);
}
const graph = new SceneGraph(), figma = new FigmaAPI(graph);
const render = ALL_TOOLS.find(t => t.name === 'render');
const source = new Uint8Array(await readFile(resolve(root, 'assets/brand/backleaf-logo-v3.png')));
const heroHash = figma.createImage(source).hash;
const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256], hashes = new Map();
for (const size of sizes) hashes.set(size, figma.createImage(new Uint8Array(await readFile(resolve(root, `assets/brand/icon-v3/${size}.png`)))).hash);

function addImage(parent, hash, size, x, y, name) {
  const node = figma.createRectangle();
  node.name = name;
  node.resize(size, size);
  node.x = x;
  node.y = y;
  node.fills = [{type: 'IMAGE', imageHash: hash, scaleMode: 'FIT'}];
  parent.appendChild(node);
}
async function addText(parent, text, x, y, size, color) {
  const jsx = `<Text name="${text}" x={${x}} y={${y}} font="Noto Sans SC" size={${size}} color="${color}">${text}</Text>`;
  await render.loadFonts(figma, {jsx});
  await editDocument(graph, () => render.execute(figma, {jsx, parent_id: parent.id}));
}
const scenes = [['overview', '#F5F7F6', '#294F43'], ['sizes-light', '#FFFFFF', '#294F43'], ['sizes-dark', '#202C28', '#E6EEE8']];
for (const [i, [name, bg, fg]] of scenes.entries()) {
  const page = i === 0 ? figma.currentPage : figma.createPage();
  figma.currentPage = page;
  page.name = name;
  const jsx = `<Frame name="Backleaf brand v3 / ${name}" w={1200} h={${i === 0 ? 800 : 480}} bg="${bg}"><Text x={40} y={30} font="Noto Sans SC" size={28} color="${fg}">Backleaf · 拾径</Text></Frame>`;
  await render.loadFonts(figma, {jsx});
  await editDocument(graph, () => render.execute(figma, {jsx}));
  const frame = figma.getNodeById(graph.getChildren(page.id)[0].id);
  frame.description = 'SIM-1030. User-adopted application icon, 2026-11-08: friendly leaf nested in a Windows-style yellow folder. Built-in image_gen raster artwork preserved, with editable presentation layout. Pixel-size samples use actual exported ICO payloads. Runtime PNG and ICO adopt v3 for preview.52. Presentation layers are editable; artwork is raster.';
  if (i === 0) {
    for (const [x, panelBg, labelColor, label] of [[40, '#FFFFFF', '#69786F', 'Light'], [616, '#202C28', '#B4C6BC', 'Dark']]) {
      const panel = figma.createFrame();
      panel.name = label;
      panel.resize(544, 420);
      panel.x = x;
      panel.y = 108;
      panel.cornerRadius = 24;
      panel.fills = [{type: 'SOLID', color: {r: parseInt(panelBg.slice(1,3),16)/255, g: parseInt(panelBg.slice(3,5),16)/255, b: parseInt(panelBg.slice(5,7),16)/255}}];
      frame.appendChild(panel);
      addImage(panel, heroHash, 360, 92, 22, 'Folder + leaf / 360px');
      await addText(panel, label, 24, 376, 14, labelColor);
    }
    await addText(frame, '16 · 24 · 32 · 48 · 64 · 128 px', 40, 566, 16, fg);
    let x = 40;
    for (const size of [16, 24, 32, 48, 64, 128]) {
      addImage(frame, hashes.get(size), size, x, 748 - size, `ICO payload / ${size}px`);
      x += size + 42;
    }
  } else {
    let x = 40;
    for (const size of sizes) {
      addImage(frame, hashes.get(size), size, x, 370 - size, `ICO payload / ${size}px`);
      await addText(frame, String(size), x, 394, 14, fg);
      x += Math.max(size, 44) + 28;
    }
  }
}
await measureDocumentText(graph);
await writeFile(file, (await io.writeDocument('fig', graph, {renderThumbnail: true})).data);
const {graph: canonical} = await io.readDocument({name: file, data: new Uint8Array(await readFile(file))});
populateAllFigPages(canonical);
await measureDocumentText(canonical);
for (const page of canonical.getPages()) {
  const node = canonical.getChildren(page.id)[0];
  await writeFile(resolve(out, page.name + '.png'), await headlessRenderNodes(canonical, page.id, [node.id], {format: 'PNG', scale: 1}));
}
console.log('Brand v3 saved: overview, sizes-light, sizes-dark');
process.exit(0);
