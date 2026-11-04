import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v23.fig'),file=resolve(root,'design/Backleaf-v24.fig'),out=resolve(root,'design/v24');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==165)throw Error('Expected 165 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:158,pages:165,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:165}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;

const surfaceVariable=figma.variables.createVariable('surface/files',collection.id,'COLOR');for(const[mode,hex]of Object.entries({Light:'#FFFFFF',Dark:'#212B35'}))surfaceVariable.setValueForMode(modes[mode],{r:parseInt(hex.slice(1,3),16)/255,g:parseInt(hex.slice(3,5),16)/255,b:parseInt(hex.slice(5,7),16)/255,a:1});
const scenes=[['159-file-surface','normal'],['160-surface-dark','dark'],['161-surface-narrow','narrow'],['162-surface-details','details'],['163-surface-marquee','marquee'],['164-surface-scrolled','scrolled'],['165-surface-maximized','maximized']];
const pages=[];
for(const[key,kind]of scenes){
 const width=kind==='narrow'?560:kind==='maximized'?1440:1120,height=kind==='narrow'?440:kind==='maximized'?900:720;
 const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v24 ');
 const original=graph.getPages().find(p=>p.name.startsWith('155 ·'));
 const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);frame.resize(width,height);frame.clipsContent=true;frame.fills=[{type:'SOLID',color:kind==='dark'?{r:.086,g:.102,b:.118}:{r:.965,g:.973,b:.98}}];
 const content=frame.children.find(n=>n.name==='Content'),files=content.children.find(n=>n.name==='Files');content.fills=[];
 const oldGroup=files.children[0];files.clipsContent=false;files.fills=[];files.layoutMode='VERTICAL';files.paddingLeft=6;files.paddingRight=6;files.paddingTop=8;files.paddingBottom=8;
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 const panel=await make(F('Primary files / radius 16 / faint static elevation','',`w="fill" h="fill" rounded={16} bg=${C('surface/files')} shadow="0 1 2 #14232B04, 0 2 8 #14232B06"`));files.appendChild(panel);panel.layoutGrow=1;panel.layoutSizingHorizontal='FILL';panel.layoutSizingVertical='FILL';panel.clipsContent=true;panel.appendChild(oldGroup);oldGroup.layoutPositioning='ABSOLUTE';oldGroup.x=0;oldGroup.y=0;
 const detail=kind==='details'?320:0,pw=width-184-12-detail,ph=height-48-16,gw=pw-36,columns=Math.max(1,Math.floor((gw+8)/108)),cell=(gw-8*(columns-1))/columns;oldGroup.resize(pw,ph);
 oldGroup.children.forEach((tile,i)=>{tile.resize(cell,112);tile.x=12+(i%columns)*(cell+8);tile.y=16+Math.floor(i/columns)*128-(kind==='scrolled'?48:0);for(const q of tile.children){if(q.name.startsWith('Artwork / '))q.x=(cell-44)/2;if(q.name==='Measured state and hit surface'){q.resize(Math.min(cell,q.width),q.height);q.x=(cell-q.width)/2;}if(q.type==='TEXT'){q.x=12;q.resize(cell-24,32);}}});
 if(detail){const d=await make(F('Details / full name',T('详情',280)+T('很长的完整文件名.pdf',280),'w={320} h="fill" px={12} py={16} flex="col" gap={16} bg='+C('surface/canvas')));content.appendChild(d);d.layoutSizingHorizontal='FIXED';d.layoutSizingVertical='FILL';}
 if(['marquee','scrolled'].includes(kind)){const rail=await make(F('Dynamic scrollbar','','w={4} h={120} rounded={2} bg='+C('content/secondary')+' opacity={0.32}'));panel.appendChild(rail);rail.layoutPositioning='ABSOLUTE';rail.x=pw-10;rail.y=kind==='scrolled'?150:16;}
 if(kind==='marquee'){const box=await make(F('Selection / clipped within content','','w={220} h={190} rounded={0} bg='+C('selection/surface')+' opacity={0.7} stroke='+C('focus/ring')+' strokeWidth={1}'));panel.appendChild(box);box.layoutPositioning='ABSOLUTE';box.x=24;box.y=100;}
 frame.setExplicitVariableModeForCollection(collection.id,modes[kind==='dark'?'Dark':'Light']);await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v24-primary-file-surface',retainedPages:158,pages,source:'scripts/design/surface-v24.mjs'},null,2));
const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);
for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}
console.log(JSON.stringify({saved:file,pages:165}));process.exit(0);
