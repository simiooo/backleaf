import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const project=resolve(import.meta.dir,'../..'),output=resolve(project,'design/v8');
const spec=JSON.parse(await readFile(resolve(project,'design/color-system.json'),'utf8'));
const io=new IORegistry(BUILTIN_IO_FORMATS),renderer=ALL_TOOLS.find(t=>t.name==='render');
const C=role=>`{designVar('Backleaf colors','${role}')}`;
// The pinned SVG importer serializes path paint expressions before importing the SVG.
// Import literal primitive paints, then bind each editable vector paint through the API.
const svgLiterals=jsx=>jsx.replace(/<path\b[^>]*>/g,path=>path.replace(/\{designVar\('Backleaf colors','([^']+)'\)\}/g,(_,role)=>JSON.stringify(spec.modes.Light[role])));
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const T=(s,{size=12,color='content/primary',weight=400,...rest}={})=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${size<=12?16:22}} weight={${weight}} color=${C(color)} ${Object.entries(rest).map(([k,v])=>`${k}={${JSON.stringify(v)}}`).join(' ')}>${esc(s)}</Text>`;
function glyph(file=false,disabled=false){return `<Frame name="Icon identity / unchanged by selection" w={44} h={44} ${disabled?'opacity={0.42}':''}><svg w={44} h={44} viewBox="0 0 48 48">${file?`<path d="M12 5h16l10 10v26a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z" fill=${C('icon/paper-fill')} stroke=${C('icon/folder-stroke')} strokeWidth={1.2}/><path d="M28 5v10h10M17 23h14M17 28h14M17 33h9" fill="none" stroke=${C('icon/folder-stroke')} strokeWidth={1.2}/>`:`<path d="M5 13a3 3 0 0 1 3-3h10l4 5h18a3 3 0 0 1 3 3v19a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3Z" fill=${C('icon/folder-fill')} stroke=${C('icon/folder-stroke')} strokeWidth={1.2}/><path d="M5 19h38" fill="none" stroke=${C('icon/folder-stroke')} strokeWidth={1}/>`}</svg></Frame>`;}
function tile(name,{selected=false,hover=false,pressed=false,focus=false,inactive=false,disabled=false,x,y,width=96}={}){
 const fill=selected?(inactive?'selection/inactive-surface':'selection/surface'):'surface/canvas';
 const fg=disabled?'content/disabled':selected?(inactive?'selection/inactive-content':'selection/content'):'content/primary';
 const long=name.length>11,label=long?'项目研究记录_最终…':name;
 return `<Frame name="File state / ${esc(name)}" ${x===undefined?'':`x={${x}} y={${y}}`} w={${width}} h={100} flex="col" items="center" gap={4} px={6} py={2} rounded={6} bg=${C(fill)}>
 ${hover||pressed?`<Rectangle name="Interaction overlay / behind content" position="absolute" x={0} y={0} w={${width}} h={100} rounded={6} bg=${C(pressed?'interaction/pressed':'interaction/hover')}/>`:''}
 <Frame name="Icon slot / no independent plate" w={56} h={56} flex="row" items="center" justify="center">${glyph(name.includes('.'),disabled)}</Frame>
 <Frame name="Name slot / stable across states" w="fill" h={36} flex="row" items="start" justify="center">
 <Frame name="Text identity / transparent" w="hug" h="hug" maxW={${width-12}} flex="col" px={4} py={2} rounded={4} ${focus?'stroke='+C('focus/ring')+' strokeWidth={2} strokeAlign="outside"':''}>
 ${T(label,{color:fg,weight:selected?600:400,...(long?{w:width-20,maxLines:2,truncate:true}:{}),textAlign:'center'})}</Frame></Frame></Frame>`;
}
const states=[['默认',{}],['悬停',{hover:true}],['按下',{pressed:true}],['已选择',{selected:true}],['选择 + 悬停',{selected:true,hover:true}],['键盘焦点',{focus:true}],['选择 + 焦点',{selected:true,focus:true}],['失焦选中',{selected:true,inactive:true}],['不可用',{disabled:true}]];
function stateBoard(name){return `<Frame name="${name}" w={1280} h={390} flex="col" px={32} py={32} gap={24} bg=${C('surface/canvas')}>
 ${T('文件与控件 · 同一套状态规则',{size:16,weight:600})}
 <Frame name="State matrix / identical geometry" w="fill" flex="row" gap={24}>${states.map(([s,props])=>`<Frame name="${s}" w={112} flex="col" items="center" gap={16}>${tile('文档',props)}${T(s,{color:'content/secondary',size:11})}</Frame>`).join('')}</Frame>
 <Frame name="Shared controls / repeated roles" w="fill" h={44} flex="row" items="center" gap={24}>
 <Frame name="Primary action" h={32} px={16} flex="row" items="center" rounded={4} bg=${C('accent/strong')}>${T('保存',{color:'accent/on-strong',weight:600})}</Frame>
 <Frame name="Hover action" h={32} px={12} flex="row" items="center" rounded={4} bg=${C('interaction/hover')}>${T('取消')}</Frame>
 <Frame name="Input focus" w={220} h={32} px={8} flex="row" items="center" rounded={4} stroke=${C('focus/ring')} strokeWidth={2}>${T('搜索文档',{color:'content/secondary'})}</Frame>
 <Frame name="Inactive action" h={32} px={12} flex="row" items="center">${T('粘贴',{color:'content/disabled'})}</Frame>
 </Frame></Frame>`;}
const controls=[['后退','m11 4-5 5 5 5'],['上一级','M9 15V3M4 8l5-5 5 5']];
function header(){return `<Frame name="Chrome / structural surface" w="fill" h={44} flex="row" items="center" pl={8} bg=${C('surface/chrome')}>
 ${controls.map(([n,p])=>`<Frame name="${n}" w={32} h={44} flex="row" items="center" justify="center"><svg w={18} h={18} viewBox="0 0 18 18"><path d="${p}" fill="none" stroke=${C('content/secondary')} strokeWidth={1.3}/></svg></Frame>`).join('')}
 <Frame h={32} flex="row" items="center" px={8}>${T('文档',{size:13})}</Frame><Frame name="Native drag region" grow={1} h={44}/>
 ${['M4 9h10','M5 5h8v8H5Z','m5 5 8 8M13 5l-8 8'].map(p=>`<Frame w={46} h={44} flex="row" items="center" justify="center"><svg w={14} h={14} viewBox="0 0 18 18"><path d="${p}" fill="none" stroke=${C('content/secondary')} strokeWidth={1}/></svg></Frame>`).join('')}</Frame>`;}
function menu(){const rows=[['打开','Enter'],['在新窗口打开','Ctrl+Enter'],null,['复制','Ctrl+C'],['剪切','Ctrl+X'],['粘贴','Ctrl+V'],null,['重命名','F2'],['移至回收站','Delete'],null,['详情','Alt+Enter']];return `<Frame name="Floating menu / visual-system specimen" position="absolute" x={320} y={140} w={248} h="hug" flex="col" px={4} py={8} rounded={6} bg=${C('surface/floating')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 8 24 #14232B1F">${rows.map((r,i)=>r?`<Frame name="Menu row" w="fill" h={30} flex="row" items="center" px={8} rounded={4} ${i===3?'bg='+C('interaction/hover'):''}>${T(r[0])}<Frame grow={1} h={1}/>${T(r[1],{size:11,color:r[0]==='粘贴'?'content/disabled':'content/secondary'})}</Frame>`:`<Frame w="fill" h={8} flex="row" items="center" px={4}><Rectangle w="fill" h={1} bg=${C('border/subtle')}/></Frame>`).join('')}</Frame>`;}
const files=['文档','图片','设计资料','研究笔记.pdf','项目研究记录_最终核对稿.pdf','预算.xlsx','归档','收据.pdf','旅行计划','个人资料','合同.pdf','参考资料','拍摄计划','会议记录.pdf','读书笔记'];
function windowScreen(name,{floating=false,marquee=false}={}){return `<Frame name="${name}" w={1120} h={720} flex="col" bg=${C('surface/canvas')} rounded={8} overflow="hidden">${header()}
 <Frame name="Canvas / one background owner" w="fill" h="fill" grow={1}>${files.map((s,i)=>tile(s,{width:1016/11,x:12+i%11*(1016/11+8),y:16+Math.floor(i/11)*116,selected:marquee?[0,1,2,11,12,13].includes(i):[0,1].includes(i)})).join('')}
 ${marquee?'<Rectangle name="Rubber band" x={6} y={8} w={300} h={224} bg="#0866DF12" stroke='+C('focus/ring')+' strokeWidth={1}/>' :''}
 ${floating?menu():''}</Frame><Frame w="fill" h={28} flex="row" items="center" px={12}>${T(marquee?'已选 6 项':'已选 2 项',{size:11,color:'content/secondary'})}</Frame></Frame>`;}
const roles=['surface/canvas','surface/chrome','surface/floating','content/primary','content/secondary','selection/surface','selection/content','focus/ring'];
const layers=`<Frame name="45 · v8 视觉系统 / 表面、语义与重复" w={1280} h={720} flex="col" px={32} py={32} gap={24} bg=${C('surface/chrome')}>
 ${T('层级、状态与节奏',{size:18,weight:600})}<Frame w="fill" h="fill" grow={1} flex="row" gap={32}>
 <Frame name="Layer demonstration / canvas then floating surface" w={784} h={540} bg=${C('surface/canvas')} rounded={8}>
 <Frame x={20} y={20} flex="row" gap={16}>${tile('文档',{selected:true})}${tile('图片')}${tile('研究笔记.pdf')}</Frame>${menu()}
 <Frame x={24} y={470} flex="row" gap={16}>${T('内容层 · 不为普通文件加卡片',{color:'content/secondary'})}</Frame></Frame>
 <Frame name="Semantic roles / bound variables" w={360} h="hug" flex="col" gap={16}>${roles.map(role=>`<Frame w="fill" h={40} flex="row" gap={12} items="center"><Rectangle w={40} h={32} rounded={4} bg=${C(role)} stroke=${C('border/subtle')} strokeWidth={1}/><Frame flex="col" gap={2}>${T(role)}${T(spec.modes.Light[role],{size:11,color:'content/secondary'})}</Frame></Frame>`).join('')}</Frame>
 </Frame></Frame>`;
const screens=[
 {key:'45-foundations',jsx:layers},
 {key:'46-light-states',jsx:stateBoard('46 · v8 状态矩阵 / 明色'),mode:'Light'},
 {key:'47-dark-states',jsx:stateBoard('47 · v8 状态矩阵 / 深色'),mode:'Dark'},
 {key:'48-selection',jsx:windowScreen('48 · v8 文件选择 / 单一浅色状态层')},
 {key:'49-floating',jsx:windowScreen('49 · v8 浮层 / 与内容层区分',{floating:true})},
 {key:'50-marquee',jsx:windowScreen('50 · v8 框选 / 同一状态语义',{marquee:true})},
];
const rgba=hex=>{const s=hex.slice(1);return {r:parseInt(s.slice(0,2),16)/255,g:parseInt(s.slice(2,4),16)/255,b:parseInt(s.slice(4,6),16)/255,a:s.length===8?parseInt(s.slice(6,8),16)/255:1};};
const luminance=hex=>{const c=rgba(hex),f=x=>x<=.04045?x/12.92:((x+.055)/1.055)**2.4;return .2126*f(c.r)+.7152*f(c.g)+.0722*f(c.b);};
const contrast=(a,b)=>{const [x,y]=[luminance(a),luminance(b)].sort((x,y)=>y-x);return (x+.05)/(y+.05);};
const checks=[];
for(const [mode,p]of Object.entries(spec.modes))for(const [fg,bg,minimum]of [['content/primary','surface/canvas',4.5],['content/secondary','surface/canvas',4.5],['content/primary','surface/floating',4.5],['content/secondary','surface/floating',4.5],['selection/content','selection/surface',4.5],['selection/inactive-content','selection/inactive-surface',4.5],['accent/on-strong','accent/strong',4.5],['focus/ring','surface/canvas',3],['focus/ring','selection/surface',3],['icon/folder-stroke','surface/canvas',3],['icon/folder-stroke','selection/surface',3]]){
 const ratio=contrast(p[fg],p[bg]);checks.push({mode,fg,bg,ratio,minimum});if(ratio<minimum)throw Error(`Contrast failed ${mode} ${fg}/${bg}: ${ratio}`);
}
await mkdir(output,{recursive:true});await writeFile(resolve(output,'contrast.json'),JSON.stringify({status:'Backleaf proposal, measured sRGB pairs',checks},null,2));
const file=resolve(project,'design/Backleaf-v8.fig'),manifestPath=resolve(output,'manifest.json');
if(process.argv.includes('--verify')){
 const bytes=new Uint8Array(await readFile(file)),{graph}=await io.readDocument({name:file,data:bytes});populateAllFigPages(graph);
 if(graph.getPages().length!==50)throw Error('Page loss');
 const manifest=JSON.parse(await readFile(manifestPath,'utf8')),codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const p of manifest.pages){const page=graph.getPages().find(x=>x.name===p.name),node=graph.getChildren(page.id)[0];if(graph.countDescendants(node.id)+1!==p.nodes)throw Error('Node loss');const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});const diff=comparePNGs(codec,new Uint8Array(await readFile(resolve(output,p.key+'.png'))),image,{threshold:.05});if(!diff||diff.changedPixels/diff.totalPixels>.001)throw Error('Cold render mismatch '+p.name);results.push({...p,changedPixels:diff.changedPixels,totalPixels:diff.totalPixels});}
 await writeFile(resolve(output,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),pages:50,added:results,result:'passed',verifiedAt:new Date().toISOString()},null,2));console.log(JSON.stringify({verified:true,pages:50,states:9,modes:2}));process.exit(0);
}
const source=resolve(project,'design/Backleaf.fig'),bytes=new Uint8Array(await readFile(source));
if(createHash('sha256').update(bytes).digest('hex')!=='505c56ce3c827f128bc75bbc85cf131826acf56a3f5614e67b5b86feb47c3d4a')throw Error('v6 changed');
const {graph}=await io.readDocument({name:source,data:bytes});populateAllFigPages(graph);const figma=new FigmaAPI(graph);await measureDocumentText(graph);
const collection=figma.variables.createVariableCollection('Backleaf colors');collection.renameMode(collection.defaultModeId,'Light');const modeIds={Light:collection.defaultModeId,Dark:collection.addMode('Dark')};
const variables={};
for(const role of Object.keys(spec.modes.Light)){const variable=figma.variables.createVariable(role,collection.id,'COLOR');variables[role]=variable;for(const [mode,p]of Object.entries(spec.modes))variable.setValueForMode(modeIds[mode],rgba(p[role]));}
function bindSvgPaints(node){
 if(node.type==='VECTOR'){
  const api=figma.getNodeById(node.id);
  for(const field of ['fills','strokes'])api[field]=api[field].map(p=>{
   if(p.type!=='SOLID')return p;
   const role=['icon/folder-fill','icon/folder-stroke','icon/paper-fill','content/secondary'].find(r=>{const c=rgba(spec.modes.Light[r]);return Math.abs(c.r-p.color.r)<.001&&Math.abs(c.g-p.color.g)<.001&&Math.abs(c.b-p.color.b)<.001;});
   return role?figma.variables.setBoundVariableForPaint(p,'color',variables[role]):p;
  });
 }
 for(const child of graph.getChildren(node.id))bindSvgPaints(child);
}
const pages=[];
for(const s of screens){const page=figma.createPage();figma.currentPage=page;const jsx=svgLiterals(s.jsx);await renderer.loadFonts(figma,{jsx});const result=await editDocument(graph,()=>renderer.execute(figma,{jsx}));if(result?.error)throw Error(JSON.stringify(result));const node=graph.getChildren(page.id)[0];bindSvgPaints(node);figma.getNodeById(node.id).setExplicitVariableModeForCollection(collection.id,modeIds[s.mode??'Light']);page.name=node.name;const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});await writeFile(resolve(output,s.key+'.png'),image);pages.push({key:s.key,name:page.name,mode:s.mode??'Light',nodes:graph.countDescendants(node.id)+1});console.log(JSON.stringify(pages.at(-1)));}
figma.currentPage=figma.root.children.find(p=>p.name===pages[1].name);const written=await io.writeDocument('fig',graph,{renderThumbnail:true});await writeFile(file,written.data);
await writeFile(manifestPath,JSON.stringify({version:'v8-semantic-colors',retainedPages:44,colorVariables:Object.keys(spec.modes.Light).length,modes:modeIds,pages,source:'scripts/design/visual-v8.mjs',status:'Research-derived proposal; supersedes rejected v7 colors'},null,2));
await writeFile(resolve(output,'source.json'),JSON.stringify(screens,null,2));console.log(JSON.stringify({saved:file,pages:50}));process.exit(0);
