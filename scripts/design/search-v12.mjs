import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const project=resolve(import.meta.dir,'../..'),out=resolve(project,'design/v12');
const file=resolve(project,'design/Backleaf-v12.fig'),base=resolve(project,'design/Backleaf-v11.fig');
const io=new IORegistry(BUILTIN_IO_FORMATS),render=ALL_TOOLS.find(t=>t.name==='render');
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const T=(s,{role='content/primary',size=13,weight=400,...rest}={})=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${size<=12?16:20}} weight={${weight}} color=${C(role)} ${Object.entries(rest).map(([k,v])=>`${k}={${JSON.stringify(v)}}`).join(' ')}>${esc(s)}</Text>`;
const F=(name,body,props='')=>`<Frame name="${name}" ${props}>${body}</Frame>`;
const spacer=n=>`<Frame h={${n}} w="fill"/>`;
const button=(label,primary=false)=>F('Action / '+label,T(label,{role:primary?'accent/on-strong':'content/primary',weight:primary?600:400}),`h={32} px={${primary?16:12}} flex="row" items="center" rounded={8} ${primary?'bg='+C('accent/strong'):''}`);
const icon=(folder=true)=>F('Stable icon identity / '+(folder?'folder':'file'),`<svg w={44} h={44} viewBox="0 0 48 48"><path d="${folder?'M5 13a3 3 0 0 1 3-3h10l4 5h18a3 3 0 0 1 3 3v19a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3Z':'M12 5h16l10 10v26H10V5Z'}" fill="#DAE5DF" stroke="#698279" strokeWidth={1.2}/></svg>`,'w={44} h={44}');
function tile(name,i,{selected=false,cut=false,editing=false,newFolder=false}={}){
 const body=F('Icon / cut dims only artwork',icon(!name.includes('.')),'w={44} h={44} '+(cut?'opacity={0.42}':''))+F('Name / centered',editing?F('Rename input / selected stem and unchanged extension',T(name,{size:12,w:88,maxLines:1,truncate:true}),'w={96} h={32} px={4} py={2} rounded={8} stroke='+C('focus/ring')+' strokeWidth={2}'):T(name,{size:12,role:cut?'content/secondary':selected?'selection/content':'content/primary',w:88,textAlign:'center',maxLines:2,truncate:true}),'w="hug" h="hug" px={4} py={2}');
 return F('File hit area / '+name,F('File visible state / content fit',body,`w="hug" h="hug" maxW={112} flex="col" items="center" gap={8} px={8} pt={8} pb={12} rounded={10} ${selected?'bg='+C('selection/surface'):''}`),`x={${12+i%8*120}} y={${16+Math.floor(i/8)*128}} w={112} h={112} flex="col" items="center"`);
}
const samples=['文档','项目研究记录_最终核对稿.pdf','预算.xlsx','图片','归档','合同.pdf','参考资料','收据.pdf','旅行计划','会议记录.pdf'];
function chrome(edit=false,error=false){return F('Custom titlebar',F('Back',T('‹',{size:20}),'w={32} h={44} flex="row" items="center" justify="center"')+F('Up',T('↑',{size:16}),'w={32} h={44} flex="row" items="center" justify="center"')+F(edit?'Path input / editable IME':'Path',T(edit?(error?'D:\\已移动的文件夹':'D:\\工作\\文档'):'文档'),`w={${edit?620:240}} h={32} px={8} flex="row" items="center" rounded={8} ${edit?'bg='+C('surface/canvas')+' stroke='+C(error?'status/negative':'focus/ring')+' strokeWidth={2}':''}`)+F('Native drag region','','grow={1} h={44}')+['−','□','×'].map(x=>F('Windows caption',T(x),'w={46} h={44} flex="row" items="center" justify="center"')).join(''),'w="fill" h={44} flex="row" items="center" pl={8} bg='+C('surface/chrome'));}
const menuRows={
 file:[['打开','Enter'],null,['复制','Ctrl+C'],['剪切','Ctrl+X'],['复制路径','Ctrl+Shift+C'],null,['重命名','F2'],['移至回收站','Delete'],null,['详情','Alt+Enter']],
 folder:[['打开','Enter'],null,['复制','Ctrl+C'],['剪切','Ctrl+X'],['粘贴到文件夹','Ctrl+V'],null,['重命名','F2'],['移至回收站','Delete'],null,['详情','Alt+Enter']],
 multi:[['复制','Ctrl+C'],['剪切','Ctrl+X'],['复制路径','Ctrl+Shift+C'],null,['重命名','F2','disabled'],['移至回收站','Delete']],
 blank:[['新建文件夹','Ctrl+Shift+N'],['粘贴','Ctrl+V','disabled'],null,['刷新','F5']],
};
function menu(kind,{x=260,y=150,active=0,disabled=false}={}){return F('Context menu / '+kind,menuRows[kind].map((r,i)=>r?F('Menu row / '+r[0],T(r[0],{role:r[2]==='disabled'?'content/disabled':'content/primary'})+F('Flexible gap','','grow={1} h={1}')+T(r[1],{role:r[2]==='disabled'?'content/disabled':'content/secondary',size:11}),`w="fill" h={32} px={8} flex="row" items="center" rounded={8} ${active===i?'bg='+C('interaction/hover'):''}`):F('Menu group break',F('Divider','','w="fill" h={1} bg='+C('border/subtle')),'w="fill" h={16} px={4} flex="row" items="center"')).join(''),`position="absolute" x={${x}} y={${y}} w={260} h="hug" flex="col" px={4} py={8} rounded={12} bg=${C('surface/floating')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 8 24 #14232B1F"`);}
function windowPage(name,{kind,cut=false,rename=false,newFolder=false,path=false,pathError=false,extra='',selected=[1],mode='Light'}={}){return F(name,chrome(path,pathError)+F('File canvas',samples.map((n,i)=>tile(newFolder&&i===0?'新建文件夹':n,i,{selected:selected.includes(i),cut:cut&&selected.includes(i),editing:(rename&&i===1)||(newFolder&&i===0)})).join('')+(kind?menu(kind):'')+extra+(pathError?F('Path error',T('找不到此位置',{role:'status/negative',size:12}),'position="absolute" x={80} y={0} w={260} h={32} px={8} py={8} rounded={8} bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1}'):''),'w="fill" h="fill" grow={1}')+F('Status',T(selected.length?'已选 '+selected.length+' 项':'10 项',{size:11,role:'content/secondary'}),'w="fill" h={28} px={12} flex="row" items="center"'),`w={1120} h={720} flex="col" bg=${C('surface/canvas')}`);}
function progress({target='文档',count='42,108 / 100,000 项',bytes='9.8 GB / 20.0 GB',phase='running',failure=false,expanded=true,operation='copy'}={}){
 const running=phase==='running'||phase==='preparing'||phase==='canceling';
 const rows=Array.from({length:failure?4:8},(_,i)=>F('Operation item / '+(i===2&&failure?'failed':i===0&&running?'active':'queued'),T(i===0?'归档_42109_项目研究报告_完整文件名称与全部补充资料_实验记录_最终核对稿.pdf':`归档_${42109+i}.pdf`,{size:13,role:i===2&&failure?'status/negative':phase==='complete'?'content/primary':i===0&&running?'accent/strong':'content/secondary',w:376,maxLines:1,truncate:true}),'w="fill" h={28} flex="row" items="center"')).join('');
 return F('Operation panel / '+operation+' / '+phase,F('Target and one lifecycle action',T(target,{weight:600,w:260,maxLines:1,truncate:true})+F('Fill','','grow={1} h={1}')+(running?button(phase==='canceling'?'取消中':'取消'):F('Close finished task / tooltip',T('×',{size:18}),'w={32} h={32} rounded={8} flex="row" items="center" justify="center"')),'w="fill" h={32} flex="row" items="center"')+spacer(8)+F('Progress track',F('Processed bytes','','w={'+(phase==='complete'?376:phase==='preparing'?44:158)+'} h={2} bg='+C('accent/strong')),'w="fill" h={2} bg='+C('border/subtle'))+spacer(8)+F('Count and bytes',F('Count column',T(count,{size:12,role:'content/secondary'}),'w={188} h={16}')+F('Bytes column',T(bytes,{size:12,role:'content/secondary'}),'w={188} h={16}'),'w="fill" h={16} flex="row" items="center"')+(expanded?spacer(16)+rows:'')+(failure?spacer(16)+F('Selected error details',T('归档_42111.pdf',{weight:600,w:376,maxLines:1,truncate:true})+spacer(8)+T('无权访问',{role:'status/negative'})+spacer(8)+F('Failure actions',button('查看位置')+(running?'':button('重试')),'w="fill" flex="row" gap={8}'),'w="fill" flex="col"'):''),'w={400} h="hug" px={12} py={16} flex="col" rounded={12} bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1} shadow="0 8 24 #14232B1F"');
}
function board(name,contents,{dark=false}={}){return F(name,contents,'w={1120} h={720} px={24} py={32} flex="row" items="start" gap={32} bg='+C('surface/canvas'));}
function conflict(){return F('Name conflict / explicit user choice',T('已存在“研究报告.pdf”',{size:16,weight:600})+spacer(16)+F('Existing and incoming metadata',T('此位置 · 12.4 MB')+spacer(8)+T('待复制 · 14.1 MB'),'w="fill" flex="col"')+spacer(24)+F('Conflict actions',button('跳过')+button('保留两者')+button('替换'),'w="fill" flex="row" gap={8}'),'w={480} h="hug" px={12} py={16} flex="col" rounded={12} bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1}');}

import {scenes} from './search-scenes.mjs';
const screens=scenes({F,T,C,spacer,tile,menu});
screens.sort((a,b)=>a.key.localeCompare(b.key));
await mkdir(out,{recursive:true});
const bytes=new Uint8Array(await readFile(process.argv.includes('--verify')?file:base));
const {graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
if(process.argv.includes('--verify')){
 const original=(await io.readDocument({name:base,data:new Uint8Array(await readFile(base))})).graph;populateAllFigPages(original);
 if(graph.getPages().length!==92)throw Error('Expected 92 pages');
 for(const [i,p]of original.getPages().entries()){const q=graph.getPages()[i];if(p.name!==q.name||original.countDescendants(p.id)!==graph.countDescendants(q.id))throw Error('Retained page changed '+p.name);}
 const manifest=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const s of manifest.pages){const page=graph.getPages().find(p=>p.name===s.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==s.nodes)throw Error('Editable layer loss');const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const diff=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,s.key+'.png'))),image,{threshold:.05});if(!diff||diff.changedPixels/diff.totalPixels>.001)throw Error('Cold render changed '+s.key);results.push({key:s.key,changedPixels:diff.changedPixels,totalPixels:diff.totalPixels});}
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:84,pages:92,result:'passed',results,verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:92,added:8}));process.exit(0);
}
const figma=new FigmaAPI(graph);await measureDocumentText(graph);
const collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const variables=figma.variables.getLocalVariables();
function bindIcons(node){
 if(node.type==='VECTOR'){
  const api=figma.getNodeById(node.id);
  for(const field of ['fills','strokes'])api[field]=api[field].map(p=>{
   if(p.type!=='SOLID')return p;
   let parent=graph.getNode(node.parentId),paper=false;while(parent&&parent.type!=='PAGE'){paper=paper||parent.name==='Stable icon identity / file';parent=graph.getNode(parent.parentId);}const role=p.color.r>.7?(paper?'icon/paper-fill':'icon/folder-fill'):'icon/folder-stroke';
   return figma.variables.setBoundVariableForPaint(p,'color',variables.find(v=>v.name===role));
  });
 }
 for(const child of graph.getChildren(node.id))bindIcons(child);
}

const pages=[];
for(const s of screens){const page=figma.createPage();figma.currentPage=page;await render.loadFonts(figma,{jsx:s.jsx});const result=await editDocument(graph,()=>render.execute(figma,{jsx:s.jsx}));if(result?.error)throw Error(JSON.stringify(result));const node=graph.getChildren(page.id)[0];bindIcons(node);figma.getNodeById(node.id).setExplicitVariableModeForCollection(collection.id,modes[s.mode??'Light']);page.name=s.name;await writeFile(resolve(out,s.key+'.png'),await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1}));pages.push({key:s.key,name:s.name,mode:s.mode??'Light',nodes:graph.countDescendants(node.id)+1});console.log(JSON.stringify(pages.at(-1)));}
figma.currentPage=figma.root.children.find(p=>p.name===pages[0].name);const written=await io.writeDocument('fig',graph,{renderThumbnail:true});await writeFile(file,written.data);
await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v12-search',retainedPages:84,pages,source:'scripts/design/search-v12.mjs',status:'Implementation proposal; native operation validation pending'},null,2));
await writeFile(resolve(out,'source.json'),JSON.stringify(screens,null,2));console.log(JSON.stringify({saved:file,pages:92}));process.exit(0);