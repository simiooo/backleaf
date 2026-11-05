import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v38.fig'),file=resolve(root,'design/Backleaf-v39.fig'),out=resolve(root,'design/v39');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==237)throw Error('Expected 237 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:233,pages:237,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:237}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const scenes=[['234-root-pending-stable','pending'],['235-applications-ready','ready'],['236-child-ready-stable','nested'],['237-bottom-flip-dark','bottom']],pages=[];
for(const[key,kind]of scenes){
 const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v39 ');
 const original=graph.getPages().find(p=>p.name.startsWith('231 ·'));
 const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);
 for(const child of [...frame.children])if(child.name.startsWith('Owned')||child.name.startsWith('Backleaf drawn /'))child.remove();
 frame.description='SIM-1001: Root menu uses a fixed application-operations row from its first frame, so asynchronous Shell discovery only changes availability, never root geometry. Application entries remain Backleaf drawn and appear in an owned submenu. Each ready column has a separate owned GPUI popup with its own bounded size, scroll and monitor placement; opening or closing a child never resizes or repositions its parent. Dynamic child contents are initialized asynchronously while the parent stays usable; create the child only when rows or a bounded error are ready. No one-row loading popup followed by a resized popup. Child fit/flip is independent of parent geometry. Keyboard left/right traverses levels, Esc dismisses family, owner close/app switch closes all, pointer gaps have grace. No recurring SetWindowPos from render. Initial native bounds equal the ready column, and text/shadows use v38 tokens.';
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 const row=(label,sub=false,pending=false)=>F('Menu row / '+label,T('',12)+T(label,184)+T(pending?'…':sub?'›':'',12,pending?'content/secondary':'content/primary'),'w="fill" h={32} px={8} flex="row" items="center" gap={8} rounded={8}');
 const sep=F('Section separation',F('Separator','','w="fill" h={1} bg='+C('border/subtle')),'w="fill" h={16} flex="row" items="center"');
 const surface=(name,body)=>F(name,body,'w={260} px={4} py={8} flex="col" gap={0} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"');
 const rootBody=['打开','复制','剪切','重命名','移至回收站','详情'].map(l=>row(l)).join('')+sep+row('应用操作',true,kind==='pending');
 const popup=await make(surface('Owned root / stable bounds',rootBody));frame.appendChild(popup);popup.layoutPositioning='ABSOLUTE';popup.x=kind==='bottom'?728:320;popup.y=kind==='bottom'?440:180;
 if(kind!=='pending'){
  const app=await make(surface('Owned applications / ready rows',row('7-Zip',true)+row('使用 Vim 编辑')+row('使用 Zed 打开')));frame.appendChild(app);app.layoutPositioning='ABSOLUTE';app.x=kind==='bottom'?460:588;app.y=kind==='bottom'?584:388;
  if(kind==='nested'||kind==='bottom'){
   const child=await make(surface('Owned child / exact initial size',row('添加到压缩包…')+row('测试压缩包')+row('复制到压缩包…')));frame.appendChild(child);child.layoutPositioning='ABSOLUTE';child.x=kind==='bottom'?192:856;child.y=kind==='bottom'?584:388;
  }
 }
 frame.setExplicitVariableModeForCollection(collection.id,modes[kind==='bottom'?'Dark':'Light']);await measureDocumentText(graph);
 pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind,rootBounds:{x:popup.x,y:popup.y,width:260,height:popup.height}});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v39-stable-menu-family',retainedPages:233,pages,source:'scripts/design/menu-stability-v39.mjs'},null,2));
const{graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:237}));process.exit(0);
