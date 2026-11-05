import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
const root=resolve(import.meta.dir,'../..'),base=resolve(root,'design/Backleaf-v43.fig'),file=resolve(root,'design/Backleaf-v44.fig'),out=resolve(root,'design/v44');
const io=new IORegistry(BUILTIN_IO_FORMATS),verify=process.argv.includes('--verify');await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(verify?file:base)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);await measureDocumentText(graph);
if(verify){
 const {graph:old}=await io.readDocument({name:base,data:new Uint8Array(await readFile(base))});populateAllFigPages(old);if(graph.getPages().length!==261)throw Error('Expected261 pages');
 for(const [i,p]of old.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||old.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed');}
 const m=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of m.pages){const page=graph.getPages().find(q=>q.name===p.name),node=graph.getChildren(page.id)[0];const rendered=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const d=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,p.key+'.png'))),rendered,{threshold:.05});await writeFile(resolve(out,p.key+'.cold.png'),rendered);if(!d||d.changedPixels)throw Error('Cold render changed '+p.key);results.push({key:p.key,changedPixels:d.changedPixels,totalPixels:d.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:256,pages:261,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log('Verified261 editable pages');process.exit(0);
}
const figma=new FigmaAPI(graph),render=ALL_TOOLS.find(t=>t.name==='render');const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w=260,role='content/primary',size=13)=>`<Text font="Noto Sans SC" size={${size}} lineHeight={20} weight={400} color=${C(role)} w={${w}} maxLines={1} truncate={true}>${s}</Text>`;
const cross='<svg w={16} h={16} viewBox="0 0 24 24"><path d="M6 6 18 18M18 6 6 18" fill="none" stroke="#6A7772" strokeWidth={1.5}/></svg>';
const scenes=[['257-new-folder-name','rename'],['258-compact-progress','progress'],['259-failed-items','failed'],['260-quiet-success','quiet'],['261-dark-progress','dark']],pages=[];
for(const[key,state]of scenes){const page=figma.createPage();figma.currentPage=page;page.name=key.replace('-',' · v44 ');const original=graph.getPages().find(p=>p.name.startsWith('248 ·'));const frame=figma.getNodeById(graph.getChildren(original.id)[0].id).clone();page.appendChild(frame);frame.description='SIM-1012: compact task feedback, no successful short-operation panel. Create-folder rename survives delayed directory notifications. Details: failed files only, conditional pagination, no unknown-size placeholders. Cancel active / dismiss ended are one action, never duplicate.';
 const make=async jsx=>{const before=new Set(page.children.map(n=>n.id));await render.loadFonts(figma,{jsx});await editDocument(graph,()=>render.execute(figma,{jsx}));return page.children.find(n=>!before.has(n.id));};
 if(state==='rename'){
  const n=await make(F('New folder / focused filename',T('新建文件夹',112),'w={128} h={36} px={8} flex="row" items="center" rounded={8} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(n);n.layoutPositioning='ABSOLUTE';n.x=248;n.y=150;
 }else if(state!=='quiet'){
  const failed=state==='failed';let body=F('Summary and single action',T(failed?'3 项未完成':'复制 → 文档',256)+cross,'w="fill" h={28} flex="row" gap={8} items="center"');
  body+=F('Progress track',F('Completed','','w={168} h={2} bg='+C('accent/strong')),'w="fill" h={2} bg='+C('border/subtle'));
  body+=T(failed?'97 / 100 项':'42,108 / 100,000 项 · 210 MB / 500 MB',296,'content/secondary',12);
  if(failed)body+=F('Only actionable failed files',T('合同草稿.pdf',296,'status/negative')+T('无法访问此文件',296,'content/secondary',12)+T('资料归档.zip',296,'status/negative')+T('目标不可用',296,'content/secondary',12)+T('其他 1 项',296,'content/secondary',12),'w="fill" flex="col" gap={4}');
  const n=await make(F('Compact operation',body,'w={320} px={12} py={16} flex="col" gap={8} rounded={12} bg='+C('surface/floating')+' shadow="0 1 2 #14232B09, 0 2 6 #14232B0B"'));frame.appendChild(n);n.layoutPositioning='ABSOLUTE';n.x=788;n.y=failed?440:588;
 }
 if(state==='dark'){const col=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');frame.setExplicitVariableModeForCollection(col.id,col.modes.find(m=>m.name==='Dark').modeId);}
 pages.push({key,name:page.name,state,nodes:graph.countDescendants(frame.id)+1});
}
figma.currentPage=figma.root.children.find(p=>p.name===pages[1].name);await measureDocumentText(graph);await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v44-compact-operations',retainedPages:256,pages},null,2));
const {graph:canonical}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(canonical);await measureDocumentText(canonical);for(const p of pages){const page=canonical.getPages().find(q=>q.name===p.name),node=canonical.getChildren(page.id)[0];await writeFile(resolve(out,p.key+'.png'),await headlessRenderNodes(canonical,page.id,[node.id],{format:'PNG',scale:1}));}console.log('Saved v44:261 pages');process.exit(0);

