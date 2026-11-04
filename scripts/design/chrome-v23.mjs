import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v22.fig'),file=resolve(root,'design/Backleaf-v23.fig'),out=resolve(root,'design/v23');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==158)throw Error('Expected 158 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:149,pages:158,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:158}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const scenes=[['150-ancestor-context','ancestor'],['151-current-context','current'],['152-edit-address','edit'],['153-blur-submit','pending'],['154-error-preserves-draft','error'],['155-volumes','volumes'],['156-narrow','narrow'],['157-dark','dark'],['158-drive-disconnected','disconnected']];
const pages=[];
for(const[key,kind]of scenes){
 const width=kind==='narrow'?560:1120,height=kind==='narrow'?440:720,mode=kind==='dark'?'Dark':'Light';
 const original=graph.getPages().find(p=>p.name.startsWith('138 ·')),page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v23 ');
 const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);frame.resize(width,height);frame.clipsContent=true;frame.children.find(n=>n.name==='Compact count')?.remove();
 const header=frame.children[0],oldPath=header.children[1],content=frame.children.find(n=>n.name==='Content'),oldSidebar=content.children[0];
 const active=['edit','pending','error'].includes(kind),pw=kind==='narrow'?160:320;
 const crumb=(text,w,selected=false)=>F('Path segment / '+text,T(text,w-8),`w={${w}} h={24} px={4} rounded={8} flex="row" items="center" ${selected?'bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"':''}`);
 const address=kind==='error'?'D:\\work\\不存在的目录':'D:\\work\\backleaf';
 let path=active?F('Editable address / draft retained',T(address,pw-16),`w={${pw}} h={32} px={8} rounded={8} flex="row" items="center" bg=${C('surface/input-active')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 1 2 #14232B0E, 0 2 6 #14232B11" ${kind==='pending'?'opacity={0.65}':''}`):F('Path / click current to edit',crumb('D:\\',36)+T('›',8,'content/secondary')+(kind==='narrow'?'':crumb('work',48,kind==='ancestor')+T('›',8,'content/secondary'))+crumb('backleaf',76,['current','dark'].includes(kind))+T('⌄',16,'content/secondary'),`w={${pw}} h={32} px={4} py={4} gap={4} rounded={10} flex="row" items="center" bg=${C('surface/input-rest')}`);
 const sidebarNames=['个人文件夹','桌面','下载','文档','图片','音乐','视频','系统 (C:)','Data (D:)','本地磁盘 (E:)','可移动磁盘 (F:)'];
 const drive=F('Disk artwork',F('Rounded disk body','',`x={1} y={2} w={16} h={13} rounded={3} stroke=${C('content/secondary')} strokeWidth={1}`)+F('Lower separator','',`x={2} y={10} w={14} h={1} bg=${C('content/secondary')}`)+F('Indicator','',`x={12} y={12} w={2} h={2} rounded={1} bg=${C('content/secondary')}`),'w={18} h={18}');
 const sidebar=F('Unified navigation / actual volume rows',sidebarNames.map((name,i)=>F('Navigation / '+name,(i<7?F('Folder artwork','','w={18} h={18}'):drive)+T(name,126),`w="fill" h={32} flex="row" items="center" gap={8} px={8} rounded={8} ${i===8?'bg='+C('selection/surface'):''}`)).join(''),'w={184} h="fill" px={8} py={16} flex="col" gap={4} bg='+C('surface/chrome'));
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 const newPath=await make(path);header.insertChild(1,newPath);oldPath.remove();
 const newSide=await make(sidebar);content.insertChild(0,newSide);oldSidebar.remove();newSide.layoutGrow=0;newSide.layoutSizingHorizontal='FIXED';newSide.layoutSizingVertical='FILL';newSide.resize(184,height-48);newSide.clipsContent=true;
 const files=content.children.find(n=>n.name==='Files'),group=files.children[0],gw=width-184-36,columns=Math.max(1,Math.floor((gw+8)/108)),cell=(gw-8*(columns-1))/columns;
 files.clipsContent=true;group.resize(width-184,height-48);group.children.forEach((tile,i)=>{tile.resize(cell,112);tile.x=12+(i%columns)*(cell+8);tile.y=16+Math.floor(i/columns)*128;for(const q of tile.children){if(q.name.startsWith('Artwork / '))q.x=(cell-44)/2;if(q.name==='Measured state and hit surface'){q.resize(Math.min(cell,q.width),q.height);q.x=(cell-q.width)/2;}if(q.type==='TEXT'){q.x=12;q.resize(cell-24,32);}}});
 const folder=figma.createImage(new Uint8Array(await readFile(resolve(root,'assets/file-types-generated/runtime/folder.png')))).hash;
 for(const row of newSide.children){const art=row.children.find(n=>n.name==='Folder artwork');if(art){const r=figma.createRectangle();r.name='Generated folder artwork';r.resize(18,18);r.fills=[{type:'IMAGE',imageHash:folder,scaleMode:'FIT'}];art.appendChild(r);}}
 const menu=['ancestor','current','dark'].includes(kind);
 if(menu){const jsx=F('Breadcrumb context / frozen target path',F('Copy file path',T('复制文件路径',156),'w="fill" h={32} px={8} rounded={8} flex="row" items="center" bg='+C('interaction/hover'))+F('Edit path',T('编辑路径',156),'w="fill" h={32} px={8} rounded={8} flex="row" items="center"'),'w={188} px={4} py={8} rounded={12} flex="col" gap={2} bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1} shadow="0 2 6 #14232B0B, 0 8 24 #14232B12"');const node=await make(jsx);frame.appendChild(node);node.layoutPositioning='ABSOLUTE';node.x=kind==='ancestor'?128:184;node.y=48;}
 if(kind==='error'||kind==='disconnected'){const text=kind==='error'?'找不到此位置':'磁盘不可用，请检查连接';const toast=await make(F('Short error toast / editor draft stays intact',T(text,260,'status/negative'),'w={280} px={10} py={14} rounded={10} flex="row" items="center" bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1} shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(toast);toast.layoutPositioning='ABSOLUTE';toast.x=80;toast.y=52;}
 frame.setExplicitVariableModeForCollection(collection.id,modes[mode]);await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v23-path-navigation-and-volumes',retainedPages:149,pages,source:'scripts/design/chrome-v23.mjs'},null,2));
const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);
for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}
console.log(JSON.stringify({saved:file,pages:158}));process.exit(0);
