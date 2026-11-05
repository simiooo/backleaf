import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v36.fig'),file=resolve(root,'design/Backleaf-v37.fig'),out=resolve(root,'design/v37');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==230)throw Error('Expected 230 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:221,pages:230,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:230}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;











const scenes=[['222-cross-window-move','cross'],['223-external-copy','copy'],['224-external-move','move'],['225-external-pin','pin'],['226-invalid-drop','invalid'],['227-outbound-files','outbound'],['228-extension-rows','menu'],['229-extension-submenu','submenu'],['230-extension-unavailable','error']],pages=[];
for(const[key,kind]of scenes){
 const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v37 ');
 const original=graph.getPages().find(p=>p.name.startsWith('216 ·')),frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);
 frame.description='Windows filesystem OLE drag in/out and across Backleaf windows. Copy/move intent follows Ctrl/Shift and volume; links/virtual unsupported formats reject explicitly. Source frozen before promotion; hover and task preflight check identities and physical ancestor cycles. Async negotiation or message-pumped completion reports actual results, never premature success or source deletion. Sidebar drop pins one folder. Esc/leave/closed window cancels previews; full selected payload or explicit resource refusal, never truncation. Third-party commands are Backleaf-drawn rows/submenus, hosted only for dynamic discovery/invocation; no native menu popup. Preserve enabled/check/radio/separator/submenu semantics, original command token and frozen source; unrepresentable owner-draw entries visibly unavailable. Invoke only on explicit choice; no fake Backleaf task completion for external commands.';
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 if(['menu','submenu','error'].includes(kind)){
  const rows=kind==='error'?T('此扩展暂不可用',220,'content/disabled'):T('打开',220)+T('复制',220)+T('剪切',220)+T('重命名',220)+F('Group break','','w="fill" h={1} bg='+C('border/subtle'))+T('7-Zip                         ›',220)+T('使用 Vim 编辑',220)+T('使用 Zed 打开',220)+T('固定到收藏夹',220,'content/disabled');
  const menu=await make(F('Backleaf drawn / registered command rows',rows,'w={244} px={12} py={16} flex="col" gap={12} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));
  frame.appendChild(menu);menu.layoutPositioning='ABSOLUTE';menu.x=320;menu.y=180;
  if(kind==='submenu'){
   const sub=await make(F('Backleaf drawn / dynamic submenu',T('添加到压缩包…',220)+T('测试压缩包',220)+T('复制到压缩包…',220),'w={244} px={12} py={16} flex="col" gap={12} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));
   frame.appendChild(sub);sub.layoutPositioning='ABSOLUTE';sub.x=568;sub.y=362;
  }
 }else{
  const label=kind==='copy'?'复制 3 项 → 文档':kind==='move'||kind==='cross'?'移动 3 项 → 文档':kind==='pin'?'固定到侧栏':kind==='invalid'?'不能移入自身的子目录':kind==='outbound'?'3 项':'正在检查目标…';
  const cue=await make(F('Drag intent / '+kind,T(label,220,kind==='invalid'?'status/negative':'content/primary'),'w={244} px={12} py={16} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));
  frame.appendChild(cue);cue.layoutPositioning='ABSOLUTE';cue.x=kind==='pin'?12:380;cue.y=kind==='pin'?340:180;
  if(kind==='cross'){
   const pair=figma.createFrame();pair.name='Two independent windows';pair.resize(2240,720);pair.fills=[];page.appendChild(pair);pair.appendChild(frame);frame.x=0;frame.y=0;const target=frame.clone();pair.appendChild(target);target.x=1120;target.y=0;
  }
 }
 await measureDocumentText(graph);const top=page.children[0];pages.push({key,name:page.name,nodes:graph.countDescendants(top.id)+1,state:kind});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v37-ole-and-custom-extensions',retainedPages:221,pages,source:'scripts/design/ole-menus-v37.mjs'},null,2));const{graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:230}));process.exit(0);
