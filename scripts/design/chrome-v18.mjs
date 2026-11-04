import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v17.fig'),file=resolve(root,'design/Backleaf-v18.fig'),out=resolve(root,'design/v18');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==129)throw Error('Expected 129 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key+' '+JSON.stringify({changed:d?.changedPixels,total:d?.totalPixels}));results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:124,pages:129,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:129}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100)=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C('content/primary')} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const names=['个人文件夹','桌面','下载','文档','图片','音乐','视频'];
const folder=figma.createImage(new Uint8Array(await readFile(resolve(root,'assets/file-types-generated/runtime/folder.png')))).hash;
const scenes=[['125-sidebar-normal',-1,3,1120,'Light'],['126-sidebar-keyboard-focus',0,3,1120,'Light'],['127-sidebar-current-focus',3,3,1120,'Light'],['128-sidebar-focus-dark',0,3,1120,'Dark'],['129-sidebar-focus-narrow',0,3,560,'Light']];
const pages=[];
for(const[key,focus,selected,width,mode]of scenes){
 const original=graph.getPages().find(p=>p.name.startsWith('113 ·')),originalRoot=graph.getChildren(original.id)[0],page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v18 ');
 const frame=figma.getNodeById(originalRoot.id).clone();page.appendChild(frame);frame.resize(width,720);frame.clipsContent=true;
 const content=frame.children.find(n=>n.name==='Content');content.children[0].remove();
 const rows=names.map((name,i)=>{
  const active=i===focus,current=i===selected;
  const surface=current?'selection/surface':active?'surface/floating':'surface/chrome';
  const foreground=current?'selection/content':'content/primary';
  return F('Sidebar row / '+name+(active?' / keyboard focus':'')+(current?' / current':''),F('Folder artwork','','w={18} h={18}')+T(name,128).replace(C('content/primary'),C(foreground)),`w="fill" h={32} flex="row" items="center" gap={8} px={8} rounded={8} bg=${C(surface)} stroke=${active?C('border/subtle'):'"#00000000"'} strokeWidth={1} ${active?'shadow="0 1 2 #14232B0E, 0 2 6 #14232B11"':''}`);
 }).join('');
 const jsx=F('Sidebar / no visible heading',rows,'w={184} h="fill" px={8} py={16} flex="col" gap={4} bg='+C('surface/chrome'));
 await render.loadFonts(figma,{jsx});const before=new Set(page.children.map(n=>n.id));await editDocument(graph,()=>render.execute(figma,{jsx}));const sidebar=page.children.find(n=>!before.has(n.id));content.insertChild(0,sidebar);sidebar.layoutGrow=0;sidebar.layoutSizingHorizontal='FIXED';sidebar.layoutSizingVertical='FILL';sidebar.resize(184,672);
 function art(n){if(n.name==='Folder artwork'){const r=figma.createRectangle();r.name='Generated rounded folder';r.resize(18,18);r.fills=[{type:'IMAGE',imageHash:folder,scaleMode:'FIT'}];figma.getNodeById(n.id).appendChild(r);}else for(const c of graph.getChildren(n.id))art(c);}art(graph.getNode(sidebar.id));
 const files=content.children.find(n=>n.name==='Files');if(files){const gridWidth=width-184-24,columns=Math.max(1,Math.floor((gridWidth+8)/108)),cell=(gridWidth-8*(columns-1))/columns;files.clipsContent=true;files.children.forEach((tile,i)=>{tile.resize(cell,112);tile.x=12+(i%columns)*(cell+8);tile.y=16+Math.floor(i/columns)*128;});}
 frame.setExplicitVariableModeForCollection(collection.id,modes[mode]);await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v18-sidebar-keyboard-focus',retainedPages:124,pages,tokens:'design/chrome-system.json',source:'scripts/design/chrome-v18.mjs'},null,2));const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0],header=canonical.getChildren(node.id)[0];if(header.height!==48)throw Error('Header height changed');await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:129}));process.exit(0);
