import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {prepareGraphFonts} from '@open-pencil/core/text';
import {measureTextHeadlessly,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v44.fig'),file=resolve(root,'design/Backleaf-v45.fig'),out=resolve(root,'design/v45');
const io=new IORegistry(BUILTIN_IO_FORMATS),exportOnly=process.argv.includes('--export'),verify=process.argv.includes('--verify')||exportOnly;await mkdir(out,{recursive:true});await measureTextHeadlessly();
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==266)throw Error('Expected266 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8'));await prepareGraphFonts(graph,m.pages.map(p=>graph.getPages().find(q=>q.name===p.name).id));
 const codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});if(exportOnly){await writeFile(resolve(out,p.key+'.png'),rendered);continue;}const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 if(exportOnly){console.log('Exported canonical v45 reference in fresh process');process.exit(0);}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:261,pages:266,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log('Verified266 editable pages');process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render');const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w,role='content/primary',size=13,line=18)=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${line}} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const art='<svg w={24} h={24} viewBox="0 0 24 24"><path d="M5 2H15L20 7V22H5Z M15 2V7H20 M9 11 7 14 9 17M16 11 18 14 16 17M14 10 11 18" fill="none" stroke="#73818A" strokeWidth={1}/></svg>';
const scenes=[['262-search-normal','normal'],['263-search-hover','hover'],['264-search-active','active'],['265-search-grouped','grouped'],['266-search-dark','dark']],pages=[];
for(const[key,state]of scenes){const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v45 ');const original=graph.getPages().find(p=>p.name.startsWith('248 ·'));const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);frame.description='SIM-1014: fixed56px list rows, 18px name +2px gap +14px parent, centered with11px vertical inset and6px horizontal inset. Grouped64px rows reserve12px heading plus2px gap. Hover and active change only surface color, never padding, font weight or geometry. Grid artwork remains centered.';
 const make=async jsx=>{const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));};
 const names=['file_icons_generated.rs','file_view.rs','headless_tests.rs','index.rs','一个非常长的文件名称用来验证省略号与容器内边距.rs','input.rs'];
 const rows=names.map((name,i)=>{const height=state==='grouped'?64:56;let content=F('Result content',art+F('Name and parent',T(name,462)+T(i===4?'src / 中文文件夹 / 一个很长的父目录': 'src',462,'content/secondary',11,14),'w="fill" flex="col" gap={2}'),'w="fill" h={34} flex="row" items="center" gap={8}');if(state==='grouped'&&i===0)content=T('Rust 源码',500,'content/secondary',11,12)+content;return F('Search result '+i,content,`w="fill" h={${height}} px={6} flex="col" justify="center" gap={2} rounded={8}`+(i===0&&state!=='normal'?' bg='+C(state==='active'?'selection/surface':'interaction/hover'):''));}).join('');
 const panel=await make(F('Search popover',T('backleaf · 含子文件夹',520,'content/secondary',12,18)+F('Virtual search results',rows,'w="fill" flex="col" gap={0}')+T('245 项',520,'content/secondary',12,18),'w={560} px={12} py={16} flex="col" gap={12} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(panel);panel.layoutPositioning='ABSOLUTE';panel.x=416;panel.y=48;
 if(state==='dark'){const col=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');frame.setExplicitVariableModeForCollection(col.id,col.modes.find(m=>m.name==='Dark').modeId);}
 pages.push({key,name:page.name,state,nodes:graph.countDescendants(frame.id)+1});
}
await prepareGraphFonts(graph,pages.map(p=>graph.getPages().find(q=>q.name===p.name).id));figma.currentPage=figma.root.children.find(p=>p.name===pages[1].name);
await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v45-search-result-rhythm',retainedPages:261,pages},null,2));
const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await prepareGraphFonts(canonical,pages.map(p=>canonical.getPages().find(q=>q.name===p.name).id));for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log('Saved v45:266 pages');process.exit(0);
