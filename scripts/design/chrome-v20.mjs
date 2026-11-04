import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v19.fig'),file=resolve(root,'design/Backleaf-v20.fig'),out=resolve(root,'design/v20');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==143)throw Error('Expected 143 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key+' '+JSON.stringify({changed:d?.changedPixels,total:d?.totalPixels}));results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:137,pages:143,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:143}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100)=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C('content/primary')} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const artworks={};for(const key of ['folder','pdf','code','image'])artworks[key]=figma.createImage(new Uint8Array(await readFile(resolve(root,'assets/file-types-generated/runtime',key+'.png')))).hash;
const samples=[['文档','folder'],['notes.md','code'],['项目归档_完整名称与补充资料_2026.pdf','pdf'],['山川.png','image'],['src','folder'],['index.ts','code']];
const scenes=[['138-short-selected',0,false,1120,720,'Light'],['139-blank-cleared',-1,false,1120,720,'Light'],['140-long-selected',2,false,1120,720,'Light'],['141-rename-input',0,true,1120,720,'Light'],['142-narrow-hits',0,false,560,440,'Light'],['143-dark-hits',0,false,1120,720,'Dark']];
const pages=[],examples=[];
for(const[key,selected,rename,width,height,mode]of scenes){
 const original=graph.getPages().find(p=>p.name.startsWith('130 ·')),originalRoot=graph.getChildren(original.id)[0],page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v20 ');
 const frame=figma.getNodeById(originalRoot.id).clone();page.appendChild(frame);frame.resize(width,height);frame.clipsContent=true;frame.children.find(n=>n.name==='Scroll overlays')?.remove();
 const content=frame.children.find(n=>n.name==='Content'),files=content.children.find(n=>n.name==='Files');for(const n of [...files.children])n.remove();files.clipsContent=true;
 const gw=width-184-36,columns=Math.max(1,Math.floor((gw+8)/108)),cell=(gw-8*(columns-1))/columns;
 const jsx=F('Content-sized hit targets',samples.map(([name,kind],i)=>{
   const long=i===2,nameWidth=long?cell-16:Math.max(24,name.length*6),nameHeight=long?36:20;
   const surfaceWidth=Math.min(cell,Math.max(44,nameWidth)+16),surfaceHeight=8+44+8+nameHeight+12;
   const active=i===selected;
   const surface=active?F('Measured state and hit surface','',`x={${(cell-surfaceWidth)/2}} y={0} w={${surfaceWidth}} h={${surfaceHeight}} rounded={10} bg=${C('selection/surface')}`):'';
   const label=rename&&i===0?F('Rename input preserves its own hitbox',T(name,76).replace('size={13}','size={12}'),`x={${(cell-Math.min(96,cell-16))/2}} y={62} w={${Math.min(96,cell-16)}} h={32} px={4} rounded={8} flex="row" items="center" bg=${C('surface/input-active')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 1 2 #14232B0E, 0 2 6 #14232B11"`):`<Text name="Measured filename / tooltip only here" x={12} y={62} w={${cell-24}} h={32} font="Noto Sans SC" size={12} lineHeight={16} weight={400} align="center" maxLines={2} truncate={true} color=${C(active?'selection/content':'content/primary')}>${name}</Text>`;
   examples.push({scene:key,name,cellWidth:cell,cellHeight:112,surfaceWidth,surfaceHeight,blankOutside:true});
   return F('Stable layout cell / '+name,surface+F('Artwork / '+kind,'',`x={${(cell-44)/2}} y={8} w={44} h={44}`)+label,`x={${12+(i%columns)*(cell+8)}} y={${16+Math.floor(i/columns)*128}} w={${cell}} h={112}`);
 }).join(''),`w={${gw+36}} h={${height-48}}`);
 await render.loadFonts(figma,{jsx});const before=new Set(page.children.map(n=>n.id));await editDocument(graph,()=>render.execute(figma,{jsx}));const group=page.children.find(n=>!before.has(n.id));files.appendChild(group);group.layoutPositioning='ABSOLUTE';group.x=0;group.y=0;group.fills=[];
 function art(n){if(n.type==='TEXT'&&n.name.startsWith('Measured filename'))figma.getNodeById(n.id).textAlignHorizontal='CENTER';if(n.name.startsWith('Artwork / ')){const r=figma.createRectangle();r.name='Preserved generated artwork';r.resize(44,44);r.fills=[{type:'IMAGE',imageHash:artworks[n.name.split(' / ')[1]],scaleMode:'FIT'}];figma.getNodeById(n.id).appendChild(r);}else for(const c of graph.getChildren(n.id))art(c);}art(graph.getNode(group.id));
 if(selected>=0){const status=F('Compact count',T('已选 1 项',64).replace('size={13}','size={12}'),`x={196} y={${height-40}} w={82} h={28} px={8} rounded={8} flex="row" items="center" bg=${C('surface/floating')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"`);const old=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx:status});await editDocument(graph,()=>render.execute(figma,{jsx:status}));const badge=page.children.find(n=>!old.has(n.id));frame.appendChild(badge);badge.layoutPositioning='ABSOLUTE';badge.x=196;badge.y=height-40;}
 frame.setExplicitVariableModeForCollection(collection.id,modes[mode]);await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v20-content-sized-file-hit',retainedPages:137,pages,tokens:'design/layout-system.json',source:'scripts/design/chrome-v20.mjs'},null,2));await writeFile(resolve(out,'hit-examples.json'),JSON.stringify({policy:'Only measured rounded identity surface is clickable; stable cell whitespace remains blank; rename input owns its own pointer events; tooltip only covers measured caption',examples},null,2));const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0],header=canonical.getChildren(node.id)[0];if(header.height!==48)throw Error('Header height changed');await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:143}));process.exit(0);
