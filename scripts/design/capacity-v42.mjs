import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v41.fig'),file=resolve(root,'design/Backleaf-v42.fig'),out=resolve(root,'design/v42');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==253)throw Error('Expected 253 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:247,pages:253,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:253}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const scenes=[['248-meter-ready','ready'],['249-meter-unknown','pending'],['250-meter-small-units','small'],['251-meter-low-space','low'],['252-meter-selected','mixed'],['253-meter-dark','dark']],pages=[];
for(const[key,kind]of scenes){
 const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v42 ');
 const original=graph.getPages().find(p=>p.name.startsWith('233 ·'));
 const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);
 frame.description='SIM-1006: static storage meter, not task progress. Volume title plus ONE caption/meter component: caption 10px/14px, component 18px, two-pixel neutral rail at bottom. Label and rail share the exact left edge and width. No extra card, percent label, status icon or animation. Summary auto scales B/KB/MB/GB/TB with Windows-compatible 1024 progression, trims .0, carries rounding across the next unit. Examples show free and total; tooltip/a11y retain used and caller-available figures. Volume row stays 60px in unknown, small, full and ready states. Existing sidebar focus/hover and overflow policy retained.';
 const content=frame.children.find(n=>n.name==='Content'),oldSide=content.children[0];frame.children.find(n=>n.name==='Compact count')?.remove();
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 const text=(value,w,size=13,role='content/primary')=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${size===10?14:20}} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${value}</Text>`;
 const common=['个人文件夹','桌面','下载','文档','图片','音乐','视频'].map(n=>F('Common / '+n,F('Folder artwork','','w={18} h={18}')+text(n,126),'w="fill" h={32} px={8} flex="row" items="center" gap={8} rounded={8}')).join('');
 const drive=F('Disk artwork',F('Rounded disk body','',`x={1} y={2} w={16} h={13} rounded={3} stroke=${C('content/secondary')} strokeWidth={1}`)+F('Lower separator','',`x={2} y={10} w={14} h={1} bg=${C('content/secondary')}`),'w={18} h={18}');
 const volumes=kind==='small'?[['Windows-SSD (C:)','75.1 GB 剩余 · 400 GB',.81],['Data (D:)','950 MB 剩余 · 461 GB',.998],['USB (F:)','640 KB 剩余 · 16 MB',.96]]:kind==='low'?[['Windows-SSD (C:)','0 B 剩余 · 400 GB',1],['Data (D:)','1 GB 剩余 · 461 GB',.998],['USB (F:)','16 MB 剩余 · 16 MB',0]]:[['Windows-SSD (C:)','75.1 GB 剩余 · 400 GB',.81],['Data (D:)','34.7 GB 剩余 · 461 GB',.925],['USB (F:)','32 GB 剩余 · 119 GB',.73]];
 const rows=volumes.map(([name,value,used],i)=>F('Volume / fixed height / '+name,drive+F('Capacity title and unified meter',text(name,126)+F('Unified capacity caption and static meter',text(kind==='pending'?'—':value,126,10,'content/secondary')+F('Usage track',kind==='pending'?'':F('Used allocation','','w={'+Math.round(126*used)+'} h={2} rounded={1} bg='+C('content/secondary')),'x={0} y={16} w={126} h={2} rounded={1} bg='+C('border/subtle')),'w={126} h={18}'),'w={126} flex="col" gap={2}'),'w="fill" h={60} px={8} py={8} flex="row" items="center" gap={8} rounded={8} '+(i===1?'bg='+C('selection/surface'):''))).join('');

 const side=await make(F('Unified navigation / volume capacity',common+rows,'w={184} h="fill" px={8} py={16} flex="col" gap={4} bg='+C('surface/chrome')));content.insertChild(0,side);oldSide.remove();side.layoutGrow=0;side.layoutSizingHorizontal='FIXED';side.layoutSizingVertical='FILL';side.resize(184,672);side.clipsContent=true;
 const folder=figma.createImage(new Uint8Array(await readFile(resolve(root,'assets/file-types-generated/runtime/folder.png')))).hash;
 for(const row of side.children){const art=row.children.find(n=>n.name==='Folder artwork');if(art){const image=figma.createRectangle();image.resize(18,18);image.fills=[{type:'IMAGE',imageHash:folder,scaleMode:'FIT'}];art.appendChild(image);}}
 const pillText=kind==='mixed'?'已选 3 项 · 1.8 MiB':kind==='folders'?'已选 2 项':'已选 1 项';
 if(kind==='mixed'||kind==='folders'){
  const wanted=kind==='mixed'?['notes.md','项目归档_完整名称与补充…']:['src'];
  const labels=frame.findAll(n=>n.type==='TEXT' && wanted.some(value=>n.characters.startsWith(value.replace('…',''))));
  for(const label of labels){
   const tile=label.parent;if(!tile || tile.width>240)continue;
   const width=Math.min(tile.width,Math.max(60,label.width+16)),height=Math.min(tile.height,label.y+label.height+8);
   const surface=await make(F('Selected identity / '+label.characters,'',`w={${width}} h={${height}} rounded={12} bg=`+C('selection/surface')));
   tile.insertChild(0,surface);surface.layoutPositioning='ABSOLUTE';surface.x=(tile.width-width)/2;surface.y=0;
  }
 }
 const pill=await make(F('Inline selection / file bytes only',text(pillText,kind==='mixed'?168:82),'h={28} px={10} flex="row" items="center" rounded={10} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(pill);pill.layoutPositioning='ABSOLUTE';pill.x=202;pill.y=668;
 frame.setExplicitVariableModeForCollection(collection.id,modes[kind==='dark'?'Dark':'Light']);await measureDocumentText(graph);
 pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v42-unified-capacity-meter',retainedPages:247,pages,source:'scripts/design/capacity-v42.mjs'},null,2));
const{graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:253}));process.exit(0);
