import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v27.fig'),file=resolve(root,'design/Backleaf-v28.fig'),out=resolve(root,'design/v28');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==186)throw Error('Expected 186 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Layer loss');const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels/d.totalPixels>.001)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:179,pages:186,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:186}));process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render'),collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=100,role='content/primary')=>`<Text font="Noto Sans SC" size={13} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;





const scenes=[['180-search-filters','filters'],['181-save-view','save'],['182-unified-view-navigation','navigation'],['183-view-unsaved-changes','modified'],['184-view-group-type','grouped'],['185-view-save-conflict','conflict'],['186-view-no-results','empty']],pages=[];
for(const[key,kind]of scenes){const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v28 ');const original=graph.getPages().find(p=>p.name.startsWith('159 ·')),frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);frame.description='Saved FileView / '+kind+'; local filename metadata filters; temporary draft distinct from save/update/as-new; same navigation entries as folders; removing rule never deletes files.';
const row=(label,value)=>F(label,T(label,70,'content/secondary')+T(value,210),'w="fill" h={32} flex="row" items="center" gap={8}');
let body=kind==='filters'?row('类型','pdf, docx')+row('最近天数','7')+row('最小字节','')+row('最大字节','')+row('范围','当前文件夹及子文件夹')+row('排序','名称')+row('分组','类型'):kind==='save'?row('名称','最近的文档')+row('','保存'):kind==='modified'?row('最近的文档','已修改')+row('','更新   另存为'):kind==='grouped'?row('PDF','研究报告.pdf')+row('DOCX','项目说明.docx'):kind==='conflict'?row('名称','最近的文档')+T('此视图已在另一个窗口修改',280,'status/negative'):kind==='empty'?T('没有匹配项',280):row('最近的文档','保存的搜索');
const jsx=F('Search / '+kind,body,'w={360} px={12} py={16} rounded={12} flex="col" gap={8} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"');const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));const node=page.children.find(n=>!before.has(n.id));frame.appendChild(node);node.layoutPositioning='ABSOLUTE';node.x=kind==='navigation'?8:widthSafe(frame.width-380);node.y=kind==='navigation'?460:56;
await measureDocumentText(graph);pages.push({key,name:page.name,nodes:graph.countDescendants(frame.id)+1,state:kind});}
function widthSafe(x){return Math.max(200,x);}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v28-saved-file-views',retainedPages:179,pages,source:'scripts/design/fileview-v28.mjs'},null,2));const{graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log(JSON.stringify({saved:file,pages:186}));process.exit(0);
