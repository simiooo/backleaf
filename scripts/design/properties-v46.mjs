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
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v45.fig'),file=resolve(root,'design/Backleaf-v46.fig'),out=resolve(root,'design/v46');
const io=new IORegistry(BUILTIN_IO_FORMATS),exportOnly=process.argv.includes('--export'),verify=process.argv.includes('--verify')||exportOnly;await mkdir(out,{recursive:true});await measureTextHeadlessly();
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==271)throw Error('Expected271 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8'));await prepareGraphFonts(graph,m.pages.map(p=>graph.getPages().find(q=>q.name===p.name).id));
 const codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});if(exportOnly){await writeFile(resolve(out,p.key+'.png'),rendered);continue;}const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 if(exportOnly){console.log('Exported canonical v46 reference in fresh process');process.exit(0);}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:266,pages:271,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log('Verified271 editable pages');process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render');const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=396,role='content/primary',size=13,line=20,lines=1)=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${line}} weight={400} color=${C(role)} w={${w}} h={${line*lines}} maxLines={${lines}}>${s}</Text>`;
const cross='<svg w={18} h={18} viewBox="0 0 24 24"><path d="M6 6 18 18M18 6 6 18" fill="none" stroke="#697781" strokeWidth={1.5}/></svg>';
const copy='<svg w={16} h={16} viewBox="0 0 24 24"><path d="M9 8H20V21H9ZM5 16H3V3H15V5" fill="none" stroke="#697781" strokeWidth={1.5}/></svg>';
const folder='<svg w={44} h={44} viewBox="0 0 48 48"><path d="M4 15Q4 10 9 10H18Q20 10 23 14H39Q44 14 44 19V35Q44 40 39 40H9Q4 40 4 35Z" fill="#9ABADA" stroke="none"/></svg>';
const fileArt='<svg w={44} h={44} viewBox="0 0 48 48"><path d="M11 4H29L38 13V43H11Z" fill="#FAFCFE" stroke="#A9B6C1" strokeWidth={1}/><path d="M29 4V13H38M17 22H32M17 27H32M17 32H27" fill="none" stroke="#A9B6C1" strokeWidth={1}/></svg>';
const scenes=[['267-file-properties','file'],['268-folder-counting','counting'],['269-folder-complete','complete'],['270-folder-partial','partial'],['271-properties-dark','dark']],pages=[];
for(const[key,state]of scenes){const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v46 ');const original=graph.getPages().find(p=>p.name.startsWith('248 ·'));const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);frame.description='SIM-1013: separate440x544 properties window, main file grid unchanged. Complete selectable full name/path, type, logical bytes, counts, creation/modification times. Folder traversal is opt-in here,2 workers/16 queued,200ms coalesced snapshots,120ms finite count tween; no reparse/offline traversal or file-content reads. Close cancels. Incomplete statistics never presented as complete. Escape/Ctrl+W close, F5 refresh. Reduced motion disables count tween.';
 const make=async jsx=>{const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));};
 const isFile=state==='file',pending=state==='counting'||state==='dark';const name=isFile?'季度研究总结与附件清单（完整保留长文件名称）.pdf':'项目资料';
 const hero=F('Item identity',(isFile?fileArt:folder)+F('Full name and kind',T(name,304,'content/primary',13,20,isFile?2:1)+T(isFile?'PDF 文件':'文件夹',304,'content/secondary',12,18),'w="fill" flex="col" gap={4}')+copy,'w="fill" flex="row" items="center" gap={12}');
 const capacity=F('Live logical size',T('大小',396,'content/secondary',12,18)+T(isFile?'2.4 MB':pending?'428 MB':'1.3 GB',396,'content/primary',24,32)+T(isFile?'2,516,582 字节':pending?'正在统计 · 2,521 个文件 · 74 个文件夹':'8,314 个文件 · 206 个文件夹',396,'content/secondary',12,18)+(state==='partial'?T('部分项目无法读取',396,'status/negative',12,18):''),'w="fill" flex="col" gap={4}');
 const field=(label,value)=>F(label,T(label,396,'content/secondary',12,18)+T(value,396,'content/primary',13,20,isFile?2:1),'w="fill" flex="col" gap={4}');
 const body=F('Scrollable properties',hero+capacity+F('Location',F('Location header',T('位置',368,'content/secondary',12,18)+copy,'w="fill" flex="row" items="center" gap={8}')+T(isFile?'D:\\work\\backleaf\\资料归档\\季度研究总结与附件清单（完整保留长文件名称）.pdf':'D:\\work\\项目资料',396,'content/primary',13,20,isFile?2:1),'w="fill" flex="col" gap={4}')+field('创建时间','2026/11/05 18:24')+field('修改时间','2026/11/05 22:31'),'w="fill" px={12} py={16} flex="col" gap={16}');
 const win=await make(F('Independent properties window',F('Titlebar',T('详情',370)+cross,'w="fill" h={44} px={12} flex="row" items="center" gap={8}')+body,'w={440} h={544} flex="col" rounded={12} bg='+C('surface/canvas')+' shadow="0 1 2 #14232B09, 0 6 18 #14232B15"'));frame.appendChild(win);win.layoutPositioning='ABSOLUTE';win.x=616;win.y=88;
 if(state==='dark'){const col=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');frame.setExplicitVariableModeForCollection(col.id,col.modes.find(m=>m.name==='Dark').modeId);}
 pages.push({key,name:page.name,state,nodes:graph.countDescendants(frame.id)+1});
}
await prepareGraphFonts(graph,pages.map(p=>graph.getPages().find(q=>q.name===p.name).id));figma.currentPage=figma.root.children.find(p=>p.name===pages[1].name);
await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v46-independent-properties',retainedPages:266,pages},null,2));
console.log('Saved v46:271 pages; export reference in a fresh process, then cold verify');process.exit(0);
