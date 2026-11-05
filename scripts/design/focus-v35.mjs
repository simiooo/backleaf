import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v34.fig'),file=resolve(root,'design/Backleaf-v35.fig'),out=resolve(root,'design/v35');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==219)throw Error('Expected 219 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:215,pages:219,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:219}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;











const scenes=[['216-keyboard-caption','light'],['217-keyboard-caption-dark','dark'],['218-invalid-input-focus','error'],['219-shell-menu','shell']],pages=[];
for(const[key,kind]of scenes){
 const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v35 ');
 const original=graph.getPages().find(p=>p.name.startsWith('159 ·')),frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);
 frame.description='Tab/Shift+Tab traverse native inputs and commit on blur; IME composition stays in input. Invalid drafts retain focus and accessible Alert. Keyboard file focus uses a raised caption, unchanged font and hit bounds. Explicit Shell menus preserve lossless DOS/UNC paths, verify identities and remain until dismissal.';
 frame.setExplicitVariableModeForCollection(collection.id,modes[kind==='dark'?'Dark':'Light']);
 if(kind==='dark')frame.fills=[{type:'SOLID',color:{r:.086,g:.102,b:.118}}];
 const files=frame.children.find(n=>n.name==='Content').children.find(n=>n.name==='Files'),tile=files.children[0].children[0].children[0];
 async function make(jsx){const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));}
 const caption=await make(F('Keyboard focus / raised caption',T('文档',40),'w={48} h={24} px={4} py={2} rounded={8} bg='+C('surface/floating')+' shadow="0 1 2 #14232B0A, 0 2 6 #14232B0D"'));
 tile.appendChild(caption);caption.layoutPositioning='ABSOLUTE';caption.x=(tile.width-48)/2;caption.y=60;
 for(const child of [...tile.children])if(child.type==='TEXT')child.remove();
 if(kind==='error'){
 const toast=await make(F('Alert / accessible name: 天数无效',T('天数无效',180,'status/negative'),'w={200} px={10} py={14} rounded={10} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));
 frame.appendChild(toast);toast.layoutPositioning='ABSOLUTE';toast.x=80;toast.y=52;
 const input=await make(F('Invalid filter / retained draft',T('-1',100),'w={124} px={12} py={16} rounded={8} bg='+C('surface/floating')+' shadow="0 1 2 #14232B0A, 0 2 6 #14232B0D"'));
 frame.appendChild(input);input.layoutPositioning='ABSOLUTE';input.x=740;input.y=60;
 }
 if(kind==='shell'){
 const menu=await make(F('Explicit compatible Shell menu',T('打开',180)+T('其他应用操作',180)+T('属性',180),'w={204} px={12} py={16} gap={12} flex="col" rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));
 frame.appendChild(menu);menu.layoutPositioning='ABSOLUTE';menu.x=340;menu.y=200;
 }
 await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v35-focus-and-input',retainedPages:215,pages,source:'scripts/design/focus-v35.mjs'},null,2));const{graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:219}));process.exit(0);
