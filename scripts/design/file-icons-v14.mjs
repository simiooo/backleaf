import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
import {icons,accents,filenames,svg} from './file-icons-spec.mjs';

const project=resolve(import.meta.dir,'../..'),out=resolve(project,'design/v14'),file=resolve(project,'design/Backleaf-v14.fig'),base=resolve(project,'design/Backleaf-v13.fig');
const io=new IORegistry(BUILTIN_IO_FORMATS),render=ALL_TOOLS.find(t=>t.name==='render');
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const escape=s=>s.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const T=(s,w=104,size=12,role='content/primary')=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${size===12?16:20}} weight={400} color=${C(role)} w={${w}} textAlign="center" maxLines={2} truncate={true}>${escape(s)}</Text>`;
const F=(name,body,props='')=>`<Frame name="${name}" ${props}>${body}</Frame>`;
const holder=(key,size=44,cut=false)=>F(`Artwork|${key}|${size}`,'',`w={${size}} h={${size}} ${cut?'opacity={0.42}':''}`);
function catalogue(dark){return F('File icon catalogue',T(dark?'文件类型 · 深色':'文件类型 · 浅色',1072,18)+icons.map((e,i)=>F('Type / '+e.key,F('44 and 24px',holder(e.key)+holder(e.key,24),'w={104} h={48} flex="row" items="end" justify="center" gap={8}')+T(e.label)+T(e.sample,104,12,'content/secondary'),`position="absolute" x={${24+i%8*134}} y={${80+Math.floor(i/8)*116}} w={112} h={104} flex="col" items="center" gap={8}`)).join(''),'w={1120} h={840} px={24} py={24} bg='+C('surface/canvas'));}
function tile(e,i,state='normal'){
 const name=e.sample;
 return F('Hit area / '+name,F('Stable identity / '+state,holder(e.key,44,state==='cut')+T(name,88,12,state==='selected'?'selection/content':'content/primary'),`w={104} h={108} px={8} pt={8} pb={12} flex="col" items="center" gap={8} rounded={10} ${state==='selected'?'bg='+C('selection/surface'):''}`),`position="absolute" x={${12+i%8*116}} y={${16+Math.floor(i/8)*128}} w={108} h={112} flex="col" items="center"`);
}
function grid(dark=false){return F('Native grid / extension identity',F('Custom titlebar',T('‹     ↑     文档',360,13)+F('Drag region','','grow={1} h={44}')+T('搜索        −      □      ×',260,13),'w="fill" h={44} flex="row" items="center" bg='+C('surface/chrome'))+F('Content',F('Quick access',T('快捷访问',160,12,'content/secondary')+['个人文件夹','桌面','下载','文档','图片','音乐','视频'].map(n=>F(n,T(n,160,13),'w="fill" h={36} flex="row" items="center"')).join(''),'w={184} h="fill" px={12} py={16} flex="col" gap={4} bg='+C('surface/chrome'))+F('Files',icons.slice(0,40).map((e,i)=>tile(e,i,i===3?'selected':i===9?'cut':'normal')).join(''),'w="fill" h="fill" grow={1} bg='+C('surface/canvas')),'w="fill" h="fill" grow={1} flex="row"'),'w={1120} h={720} flex="col" bg='+C('surface/canvas'));}
const states=F('State and small size rhythm',T('文件类型图标 · 状态与小尺寸',1072,18)+['normal','selected','cut'].map((s,i)=>F('State / '+s,T(s,320,13)+F('Same artwork / no selected recolor',[icons[2],icons[8],icons[16]].map((e,j)=>tile(e,j,s)).join(''),'w={352} h={160}'),`position="absolute" x={${24+i*360}} y={88} w={352} h={240} flex="col" gap={16}`)).join('')+F('Search rows / 24px',icons.slice(2,14).map(e=>F('Result / '+e.sample,holder(e.key,24)+T(e.sample,280,13)+T('此文件夹',160,12,'content/secondary'),'w={520} h={40} flex="row" items="center" gap={8}')).join(''),'position="absolute" x={300} y={296} w={544} h={504} px={12} py={12} flex="col" rounded={12} bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1}'),'w={1120} h={840} px={24} py={24} bg='+C('surface/canvas'));
const screens=[['102-icons-light','102 · v14 文件类型图标 / 浅色',catalogue(false),'Light'],['103-icons-dark','103 · v14 文件类型图标 / 深色',catalogue(true),'Dark'],['104-grid-light','104 · v14 文件类型网格 / 浅色',grid(),'Light'],['105-grid-dark','105 · v14 文件类型网格 / 深色',grid(true),'Dark'],['106-icon-states','106 · v14 图标状态与搜索小尺寸',states,'Light']];
await mkdir(out,{recursive:true});
const verify=process.argv.includes('--verify'),bytes=new Uint8Array(await readFile(verify?file:base));
const {graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(verify){
 const {graph:original}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(original);
 if(graph.getPages().length!==106)throw Error('Page loss');
 for(const [i,p]of original.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||original.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed '+p.name);}
 const manifest=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of manifest.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Editable layer loss');const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const diff=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),image,{threshold:.05});if(!diff||diff.changedPixels/diff.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:diff.changedPixels,totalPixels:diff.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:101,pages:106,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:106,types:icons.length}));process.exit(0);
}
const figma=new FigmaAPI(graph),spec=JSON.parse(await readFile(resolve(project,'design/color-system.json'),'utf8'));
const collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors'),modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const rgba=h=>({r:parseInt(h.slice(1,3),16)/255,g:parseInt(h.slice(3,5),16)/255,b:parseInt(h.slice(5,7),16)/255,a:1});
for(const [name,values]of Object.entries(accents)){const v=figma.variables.createVariable('file-type/'+name,collection.id,'COLOR');for(const [i,mode]of ['Light','Dark'].entries())v.setValueForMode(modes[mode],rgba(values[i]));}
const vars=figma.variables.getLocalVariables();
function bind(node,dark){
 if(node.type==='VECTOR'){
  const api=figma.getNodeById(node.id),mode=dark?'Dark':'Light';
  const colors={...Object.fromEntries(['icon/folder-fill','icon/folder-stroke','icon/paper-fill'].map(r=>[r,spec.modes[mode][r]])),...Object.fromEntries(Object.entries(accents).map(([k,v])=>['file-type/'+k,v[dark?1:0]]))};
  for(const field of ['fills','strokes'])api[field]=api[field].map(p=>{if(p.type!=='SOLID')return p;const role=Object.keys(colors).find(r=>{const c=rgba(colors[r]);return Math.abs(c.r-p.color.r)<.001&&Math.abs(c.g-p.color.g)<.001&&Math.abs(c.b-p.color.b)<.001;});return role?figma.variables.setBoundVariableForPaint(p,'color',vars.find(v=>v.name===role)):p;});
 }
 for(const c of graph.getChildren(node.id))bind(c,dark);
}
const pages=[];
for(const [key,name,jsx,mode]of screens){
 const page=figma.createPage();figma.currentPage=page;page.name=name;await render.loadFonts(figma,{jsx});const result=await editDocument(graph,()=>render.execute(figma,{jsx}));if(result?.error)throw Error(JSON.stringify(result));
 const node=graph.getChildren(page.id)[0],holders=[];
 function scan(n){if(n.name.startsWith('Artwork|'))holders.push(n);for(const c of graph.getChildren(n.id))scan(c);}scan(node);
 for(const h of holders){const[,key,size]=h.name.split('|'),entry=icons.find(i=>i.key===key),art=figma.createNodeFromSvg(svg(entry,mode==='Dark').replaceAll('="96"',`="${size}"`));art.name='Vector artwork / '+key;art.fills=[];figma.getNodeById(h.id).appendChild(art);art.x=0;art.y=0;bind(graph.getNode(art.id),mode==='Dark');}
 figma.getNodeById(node.id).setExplicitVariableModeForCollection(collection.id,modes[mode]);await measureDocumentText(graph);
 await writeFile(resolve(out,key+'.png'),await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1}));pages.push({key,name,mode,nodes:graph.countDescendants(node.id)+1});console.log(JSON.stringify(pages.at(-1)));
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);
await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v14-file-icons',retainedPages:101,pages,types:icons.length,source:'scripts/design/file-icons-v14.mjs'},null,2));
await writeFile(resolve(out,'coverage.json'),JSON.stringify({icons,filenames,extensionCount:icons.reduce((n,i)=>n+i.extensions.length,0)},null,2));
await mkdir(resolve(out,'icons'),{recursive:true});for(const i of icons)for(const dark of [false,true])await writeFile(resolve(out,'icons',`${i.key}${dark?'-dark':''}.svg`),svg(i,dark));
console.log(JSON.stringify({saved:file,pages:106,types:icons.length}));process.exit(0);
