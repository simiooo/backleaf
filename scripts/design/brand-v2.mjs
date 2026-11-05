import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {SceneGraph} from '@open-pencil/scene-graph';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),out=resolve(root,'design/brand-v2'),file=resolve(root,'design/Backleaf-brand-v2.fig');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
if(verify){
 const bytes=new Uint8Array(await readFile(file)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
 const codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 if(graph.getPages().length!==3)throw Error('Expected three brand pages');
 for(const page of graph.getPages()){
  const node=graph.getChildren(page.id)[0],png=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});
  const difference=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,page.name+'.png'))),png,{threshold:.05});
  if(!difference||difference.changedPixels!==0)throw Error('Cold reopen changed '+page.name);
  results.push({page:page.name,nodes:graph.countDescendants(node.id)+1,changedPixels:difference.changedPixels});
 }
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),pages:3,results,result:'passed'},null,2));console.log('Brand v2 cold reopen passed');process.exit(0);
}
const graph=new SceneGraph(),figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render');
const asset=new Uint8Array(await readFile(resolve(root,'assets/brand/backleaf-logo-v2.png'))),hash=figma.createImage(asset).hash;
const scenes=[['light','#FFFFFF','#294F43'],['dark','#202C28','#E6EEE8'],['sizes','#F5F7F6','#294F43']];
for(const [i,[name,bg,fg]]of scenes.entries()){
 const page=i===0?figma.currentPage:figma.createPage();figma.currentPage=page;page.name=name;
 const jsx=`<Frame name="Backleaf brand v2 / ${name}" w={960} h={480} bg="${bg}"><Text x={32} y={24} font="Noto Sans SC" size={16} color="${fg}">Backleaf · 圆叶</Text></Frame>`;
 await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));
 const frame=figma.getNodeById(graph.getChildren(page.id)[0].id);frame.description='Built-in image_gen raster source; placement and presentation frames editable. Rounded leaf character with two eyes and a smile. No sharp fold or path cut. Preserve generated alpha; no tile baked into artwork. PNG-derived ICO sizes 16/20/24/32/40/48/64/128/256. This is a brand asset companion, not a replacement for the 247-page UI master.';
 const sizes=name==='sizes'?[16,20,24,32,40,48,64,128,256]:[320];
 let x=name==='sizes'?32:320;
 for(const size of sizes){
  const image=figma.createRectangle();image.name=`Generated rounded leaf / ${size}px`;image.resize(size,size);image.x=x;image.y=name==='sizes'?112:88;image.fills=[{type:'IMAGE',imageHash:hash,scaleMode:'FIT'}];frame.appendChild(image);
  x+=size+20;
 }
}
await measureDocumentText(graph);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);
const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);
for(const page of canonical.getPages()){const node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,page.name+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}
console.log('Brand v2 saved: three editable presentation pages');process.exit(0);
