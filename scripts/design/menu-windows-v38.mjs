import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v37.fig'),file=resolve(root,'design/Backleaf-v38.fig'),out=resolve(root,'design/v38');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==233)throw Error('Expected 233 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:230,pages:233,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:233}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;











const scenes=[['231-owned-context-window','main'],['232-owned-submenu-window','submenu'],['233-menu-focus-return','dismissed']],pages=[];
for(const[key,kind]of scenes){const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v38 ');const original=graph.getPages().find(p=>p.name.startsWith(kind==='submenu'?'229 ·':kind==='dismissed'?'216 ·':'228 ·')),frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);
frame.description='Context menus are separate borderless GPUI top-level windows owned by their source window, not rendered inside its scrolling/clipping surface. They are transient tool windows, not taskbar applications or globally topmost. Parent close/minimize/app switch dismisses the menu family. Click outside/Esc dismisses and returns keyboard focus to the owner when appropriate. Each dynamic submenu may use its own owned window; arrow keys cross levels, pointer gaps use a short grace period, monitor work-area flipping and DPI are resolved per window. Third-party rows remain Backleaf drawn; isolated Shell host only discovers/initializes/invokes. Frozen menu context never switches to current selection or a replacement file.';
if(kind!=='dismissed'){
 for(const child of [...frame.children])if(child.name.startsWith('Backleaf drawn /'))child.remove();
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 const row=(label,sub=false,check='')=>F('Menu row / '+label,T(check,12)+T(label,184)+T(sub?'›':'',12),'w="fill" h={32} px={8} flex="row" items="center" gap={8} rounded={8}');
 const body=['打开','复制','剪切','重命名','移至回收站','详情'].map(l=>row(l)).join('')+F('Section separation',F('Separator','','w="fill" h={1} bg='+C('border/subtle')),'w="fill" h={16} flex="row" items="center"')+row('7-Zip',true)+row('使用 Vim 编辑')+row('使用 Zed 打开');
 const popup=await make(F('Owned GPUI popup window',body,'w={260} px={4} py={8} flex="col" gap={0} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(popup);popup.layoutPositioning='ABSOLUTE';popup.x=320;popup.y=180;
 if(kind==='submenu'){const sub=await make(F('Owned popup / dynamic submenu',row('添加到压缩包…')+row('测试压缩包')+row('复制到压缩包…'),'w={260} px={4} py={8} flex="col" gap={0} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(sub);sub.layoutPositioning='ABSOLUTE';sub.x=588;sub.y=396;}
}
await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind});}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v38-owned-menu-windows',retainedPages:230,pages,source:'scripts/design/menu-windows-v38.mjs'},null,2));const{graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:233}));process.exit(0);
