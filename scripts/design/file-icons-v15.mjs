import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
import {normalizeIcon} from './normalize-icon.mjs';

export const generatedKeys=['folder','word','spreadsheet','presentation','pdf','application','installer','image','video','audio','archive','book','font','code','generic'];
export const rasterKey=key=>generatedKeys.includes(key)?key:key==='text'?'generic':'code';
if(import.meta.main){
 const root=resolve(import.meta.dir,'../..'),out=resolve(root,'design/v15'),file=resolve(root,'design/Backleaf-v15.fig'),base=resolve(root,'design/Backleaf-v14.fig');
 const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
 const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
 if(verify){
  const {graph:original}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(original);
  if(graph.getPages().length!==112)throw Error('Page loss');for(const [i,p]of original.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||graph.countDescendants(p.id)!==original.countDescendants(p.id))throw Error('Retained page changed '+p.name);}
  const manifest=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
  for(const p of manifest.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const diff=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),image,{threshold:.05});if(!diff||diff.changedPixels/diff.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:diff.changedPixels,totalPixels:diff.totalPixels});}
  await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:106,pages:112,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:112,generatedAssets:15}));process.exit(0);
 }
 const figma=new FigmaAPI(graph),images={},optical=[];await mkdir(resolve(root,'assets/file-types-generated/runtime'),{recursive:true});
 for(const key of generatedKeys){const raw=new Uint8Array(await readFile(resolve(root,'assets/file-types-generated/source',key+'.png'))),normalized=await normalizeIcon(raw,192,{cleanMatte:true});images[key]=figma.createImage(normalized.bytes).hash;await writeFile(resolve(root,'assets/file-types-generated/runtime',key+'.png'),normalized.bytes);optical.push({key,matte:normalized.matte,sourceBounds:normalized.sourceBounds,displayInkBox:normalized.displayInkBox,slot:44,inkLimit:40,sourceSha256:createHash('sha256').update(raw).digest('hex'),runtimeSha256:createHash('sha256').update(normalized.bytes).digest('hex')});}
 images.native=figma.createImage((await normalizeIcon(new Uint8Array(await readFile(resolve(root,'assets/brand/backleaf-logo.png'))))).bytes).hash;
 function place(parent,key,size){const r=figma.createRectangle();r.name='Generated raster / '+key;r.resize(size,size);r.fills=[{type:'IMAGE',imageHash:images[key],scaleMode:'FIT'}];parent.appendChild(r);r.x=0;r.y=0;return r;}
 const pages=[];
 const records=[['107-ai-icons-light',102,'107 · v15 生图文件图标 / 浅色'],['108-ai-icons-dark',103,'108 · v15 生图文件图标 / 深色'],['109-ai-grid-light',104,'109 · v15 生图文件网格 / 浅色'],['110-ai-grid-dark',105,'110 · v15 生图文件网格 / 深色'],['111-ai-states',106,'111 · v15 生图图标状态与小尺寸']];
 for(const [key,number,name]of records){
  const old=graph.getPages().find(p=>p.name.startsWith(number+' ·')),original=graph.getChildren(old.id)[0],page=figma.createPage();page.name=name;figma.currentPage=page;
  const frame=figma.getNodeById(original.id).clone();page.appendChild(frame);const holders=[],sidebarRows=[];function walk(n){if(n.name.startsWith('Artwork|'))holders.push(n);if(['个人文件夹','桌面','下载','文档','图片','音乐','视频'].includes(n.name)&&n.type==='FRAME'&&n.height===36)sidebarRows.push(n);for(const c of graph.getChildren(n.id))walk(c);}walk(graph.getNode(frame.id));
  for(const h of holders){const[,kind,size]=h.name.split('|'),key=rasterKey(kind);const parent=figma.getNodeById(h.id);if(!key){const art=parent.children[0];art.rescale(48/44);art.x=-Number(size)*2/44;art.y=-Number(size)*3/44;continue;}for(const c of [...parent.children])c.remove();place(parent,key,Number(size));}
  for(const node of sidebarRows){const row=figma.getNodeById(node.id);for(const child of row.children)if(child.type==='TEXT'){child.resize(128,20);child.textAlignHorizontal='LEFT';}row.itemSpacing=8;row.primaryAxisAlignItems='MIN';const artwork=place(row,'folder',18);row.insertChild(0,artwork);}
  await writeFile(resolve(out,key+'.png'),await headlessRenderNodes(graph,page.id,[frame.id],{format:'PNG',scale:1}));pages.push({key,name,nodes:graph.countDescendants(frame.id)+1});
 }
 const page=figma.createPage();page.name='112 · v15 EXE 原生图标 / 读取前、成功、回退';figma.currentPage=page;
 const render=ALL_TOOLS.find(t=>t.name==='render'),jsx='<Frame name="Executable artwork states" w={1120} h={480} bg="#FFFFFF"><Text font="Noto Sans SC" x={24} y={24} size={18}>EXE · 原生图标优先</Text>'+['读取前','资源已读取','读取失败'].map((s,i)=>`<Frame name="State ${s}" x={${100+i*340}} y={112} w={240} h={240} flex="col" items="center" gap={24}><Text font="Noto Sans SC" size={13}>${s}</Text><Frame name="ExeArtwork|${i===1?'native':'application'}" w={44} h={44}/><Text font="Noto Sans SC" size={12}>Backleaf.exe</Text></Frame>`).join('')+'</Frame>';
 await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));const frame=graph.getChildren(page.id)[0];function fill(n){if(n.name.startsWith('ExeArtwork|'))place(figma.getNodeById(n.id),n.name.split('|')[1],44);for(const c of [...graph.getChildren(n.id)])if(!c.name.startsWith('Generated raster'))fill(c);}fill(frame);await measureDocumentText(graph);
 await writeFile(resolve(out,'112-exe-states.png'),await headlessRenderNodes(graph,page.id,[frame.id],{format:'PNG',scale:1}));pages.push({key:'112-exe-states',name:page.name,nodes:graph.countDescendants(frame.id)+1});
 await writeFile(resolve(root,'assets/file-types-generated/optical-sizing.json'),JSON.stringify({policy:'alpha-bound crop; preserve aspect ratio; visible artwork within 40px in the unchanged 44px slot; original files retained',icons:optical},null,2));
 figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);
 await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v15-generated-file-icons',retainedPages:106,pages,generatedAssets:generatedKeys,source:'scripts/design/file-icons-v15.mjs'},null,2));console.log(JSON.stringify({saved:file,pages:112,generatedAssets:15}));process.exit(0);
}
