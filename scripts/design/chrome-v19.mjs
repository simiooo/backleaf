import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v18.fig'),file=resolve(root,'design/Backleaf-v19.fig'),out=resolve(root,'design/v19');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==137)throw Error('Expected 137 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key+' '+JSON.stringify({changed:d?.changedPixels,total:d?.totalPixels}));results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:129,pages:137,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:137}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100)=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C('content/primary')} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const tokens=JSON.parse(await readFile(resolve(root,'design/scroll-system.json'),'utf8'));
const scenes=[['130-fit-no-scroll',1120,720,7,8,false,0,'Light'],['131-main-overflow',1120,720,7,47,false,.25,'Light'],['132-sidebar-overflow-hidden',1120,440,24,47,false,0,'Light'],['133-sidebar-overflow-hover',1120,440,24,47,true,.3,'Light'],['134-short-window',560,440,24,47,true,.8,'Light'],['135-scroll-dark',1120,720,24,47,true,.45,'Dark'],['136-ten-million-scroll',1120,720,7,10000000,false,.75,'Light'],['137-empty-no-scroll',560,440,0,0,false,0,'Light']];
const pages=[];
for(const[key,width,height,sideCount,fileCount,hover,position,mode]of scenes){
 const original=graph.getPages().find(p=>p.name.startsWith('125 ·')),originalRoot=graph.getChildren(original.id)[0],page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v19 ');
 const frame=figma.getNodeById(originalRoot.id).clone();page.appendChild(frame);frame.resize(width,height);frame.clipsContent=true;
 const content=frame.children.find(n=>n.name==='Content'),sidebar=content.children[0],files=content.children.find(n=>n.name==='Files');sidebar.clipsContent=true;
 const existing=[...sidebar.children];for(let i=sideCount;i<existing.length;i++)existing[i].remove();
 for(let i=existing.length;i<sideCount;i++){const row=existing[i%existing.length].clone();sidebar.appendChild(row);row.name='Extra quick entry '+i;}
 const gridWidth=width-184-24-tokens.gutter,columns=Math.max(1,Math.floor((gridWidth+8)/108)),cell=(gridWidth-8*(columns-1))/columns,visibleHeight=height-48-32;
 for(const tile of [...files.children].slice(fileCount))tile.remove();
 files.clipsContent=true;files.children.forEach((tile,i)=>{tile.resize(cell,112);tile.x=12+(i%columns)*(cell+8);tile.y=16+Math.floor(i/columns)*128;});
 frame.setExplicitVariableModeForCollection(collection.id,modes[mode]);
 const mainContent=Math.ceil(fileCount/columns)*128,sideContent=Math.max(0,sideCount*36-4);
 const bars=[];function bar(name,x,contentHeight,show){if(!show||contentHeight<=visibleHeight)return;const length=Math.min(visibleHeight,Math.max(tokens.minimumThumbHeight,visibleHeight*visibleHeight/contentHeight));bars.push(F(name,F('Proportional thumb','',`x={4} y={${(visibleHeight-length)*position}} w={4} h={${length}} rounded={2} bg=${C('content/secondary')} opacity={0.42}`),`x={${x}} y={64} w={12} h={${visibleHeight}}`));}
 bar('Full directory scrollbar',width-14,mainContent,true);bar('Sidebar hover scrollbar',170,sideContent,hover);
 const badge=fileCount>0?F('Compact selection count',T('已选 18 项',70).replace('size={13}','size={12}').replace('lineHeight={20}','lineHeight={18}'),`x={196} y={${height-12-tokens.statusHeight}} w={88} h={28} px={8} flex="row" items="center" rounded={8} bg=${C('surface/floating')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"`):'';
 const jsx=F('Scroll overlays',bars.join('')+badge,`w={${width}} h={${height}}`);await render.loadFonts(figma,{jsx});const before=new Set(page.children.map(n=>n.id));await editDocument(graph,()=>render.execute(figma,{jsx}));const overlay=page.children.find(n=>!before.has(n.id));frame.appendChild(overlay);overlay.layoutPositioning='ABSOLUTE';overlay.x=0;overlay.y=0;overlay.fills=[];
 await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v19-responsive-scrollbars',retainedPages:129,pages,tokens:'design/scroll-system.json',source:'scripts/design/chrome-v19.mjs'},null,2));const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0],header=canonical.getChildren(node.id)[0];if(header.height!==48)throw Error('Header height changed');await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:137}));process.exit(0);
