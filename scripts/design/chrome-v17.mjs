import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v16.fig'),file=resolve(root,'design/Backleaf-v17.fig'),out=resolve(root,'design/v17');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==124)throw Error('Expected 124 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key+' '+JSON.stringify({changed:d?.changedPixels,total:d?.totalPixels}));results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:120,pages:124,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:124}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const contourVariable=figma.variables.getLocalVariables().find(v=>v.name==='icon/paper-contour');for(const [mode,hex]of Object.entries({Light:'#A9B6C1',Dark:'#9BAAB8'}))contourVariable.setValueForMode(modes[mode],{r:parseInt(hex.slice(1,3),16)/255,g:parseInt(hex.slice(3,5),16)/255,b:parseInt(hex.slice(5,7),16)/255,a:1});
function bindContour(node){if(node.type==='VECTOR'){const api=figma.getNodeById(node.id);api.strokes=api.strokes.map(p=>figma.variables.setBoundVariableForPaint(p,'color',contourVariable));}for(const child of graph.getChildren(node.id))bindContour(child);}
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100)=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C('content/primary')} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const glyphs={back:'m11 4-5 5 5 5',up:'M9 15V3M4 8l5-5 5 5',search:'M12 8a5 5 0 1 1-10 0 5 5 0 1 1 10 0m-1 4 5 5',min:'M4 9h10',max:'M5 5h8v8H5Z',close:'m5 5 8 8m0-8-8 8',more:'m5 7 4 4 4-4'};
const icon=k=>`<svg w={18} h={18} viewBox="0 0 18 18"><path d="${glyphs[k]}" fill="none" stroke="#596975" strokeWidth={1.3}/></svg>`;
function control(key,state='',width=32){
 const shadow=state==='focus'?'0 1 2 #14232B0E, 0 2 6 #14232B11':'0 1 2 #14232B09, 0 2 6 #14232B0B';
 return F('Control / '+key+' / '+(state||'normal'),icon(key),`w={${width}} h={32} rounded={8} flex="row" items="center" justify="center" ${state?'bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1} shadow="'+shadow+'"':''}`);
}
function bar({state='',progress=0,width=1120,dark=false}={}){
 const expanded=progress>0,searchWidth=expanded?32+(Math.min(320,width<700?200:320)-32)*progress:32;
 const pathWidth=width<700?(expanded?96:240):320;
 const crumbs=width<700?T('Documents',64):['C:\\','Users','qq651','Documents'].map((s,i)=>F('Crumb / '+s,T(s,i===3?88:i===0?28:44),`h={24} px={4} rounded={8} flex="row" items="center" ${state==='crumb'&&i===1?'bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"':''}`)+(i===3?'':T('›',12))).join('');
 return F('Chrome / 48px and 8px clearance',F('Navigation buttons',control('back',state==='focus'?'focus':'')+control('up',state==='hover'?'hover':''),'w="hug" h={32} flex="row" gap={8}')+F('Padded breadcrumb',crumbs+F('Click-only dropdown',icon('more'),'w={24} h={24} flex="row" items="center" justify="center" rounded={8}'),'w={'+pathWidth+'} h={32} px={4} py={4} flex="row" items="center" rounded={10} bg='+C('surface/input-rest'))+F('Native drag region','','grow={1} minW={16} h={32}')+(expanded?F('Expanded search / one surface',icon('search')+F('Closing label / fade with width',T('合同',Math.max(0,searchWidth-44)),`w={${Math.max(0,searchWidth-44)}} h={20} flex="row" items="center" opacity={${progress}}`),`w={${searchWidth}} h={32} px={8} flex="row" items="center" gap={8} rounded={8} bg=${C('surface/floating')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"`):control('search',state==='search'?'hover':''))+F('Windows caption controls',control('min','',40)+control('max','',40)+control('close','',40),'w="hug" h={32} flex="row" gap={4}'),'w="fill" h={48} px={8} py={8} flex="row" items="center" gap={8} bg='+C('surface/chrome'));
}
const scenes=[['121-search-close-start','',1,1120,'Light'],['122-search-close-quarter','',.421875,1120,'Light'],['123-search-close-end','',.015625,560,'Light'],['124-search-closed','',0,1120,'Light']];
const pages=[];
for(const[key,state,progress,width,mode]of scenes){
 const original=graph.getPages().find(p=>p.name.startsWith(mode==='Dark'?'110 ·':'109 ·')),originalRoot=graph.getChildren(original.id)[0],page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v17 ');
 const frame=figma.getNodeById(originalRoot.id).clone();page.appendChild(frame);frame.resize(width,720);frame.clipsContent=true;frame.children[0].remove();
 const jsx=bar({state,progress,width,dark:mode==='Dark'});await render.loadFonts(figma,{jsx});const before=new Set(page.children.map(n=>n.id));await editDocument(graph,()=>render.execute(figma,{jsx}));const header=page.children.find(n=>!before.has(n.id));frame.insertChild(0,header);header.layoutGrow=0;header.layoutSizingVertical='FIXED';header.layoutSizingHorizontal='FILL';header.resize(width,48);
 const content=frame.children.find(n=>n.name==='Content'),files=content?.children.find(n=>n.name==='Files');if(files){const gridWidth=width-184-24,columns=Math.max(1,Math.floor((gridWidth+8)/108)),cell=(gridWidth-8*(columns-1))/columns;files.clipsContent=true;files.children.forEach((tile,i)=>{tile.resize(cell,112);tile.x=12+(i%columns)*(cell+8);tile.y=16+Math.floor(i/columns)*128;});}
 // A separate renderer contour stays legible when the AI bitmap's hairline
 // is smaller than one display pixel. The generated artwork remains intact.
 const holders=[];function visit(n){if(n.name.startsWith('Artwork|'))holders.push(n);for(const c of graph.getChildren(n.id))visit(c);}visit(graph.getNode(frame.id));
 for(const h of holders){const[,kind,slot]=h.name.split('|');if(kind==='folder')continue;const s=Number(slot),overlay=figma.createNodeFromSvg(`<svg xmlns="http://www.w3.org/2000/svg" width="${s}" height="${s}" viewBox="0 0 44 44"><path d="M8 2.4H28L37.6 12v27.6q0 2-2 2H8q-2 0-2-2V4.4q0-2 2-2Z" fill="none" stroke="#A9B6C1" stroke-width="0.8" stroke-linejoin="round"/></svg>`);overlay.fills=[];overlay.name='Small-size continuous paper contour';figma.getNodeById(h.id).appendChild(overlay);overlay.x=0;overlay.y=0;bindContour(graph.getNode(overlay.id));}
 frame.setExplicitVariableModeForCollection(collection.id,modes[mode]);await measureDocumentText(graph);await writeFile(resolve(out,key+'.png'),await headlessRenderNodes(graph,page.id,[frame.id],{format:'PNG',scale:1}));pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v17-search-close',retainedPages:120,pages,tokens:'design/chrome-system.json',source:'scripts/design/chrome-v17.mjs'},null,2));const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0],header=canonical.getChildren(node.id)[0];if(header.height!==48)throw Error('Header height changed');await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:124}));process.exit(0);
