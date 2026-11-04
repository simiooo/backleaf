import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

const root=resolve(import.meta.dir,'../..'),out=resolve(root,'design/v7');
const io=new IORegistry(BUILTIN_IO_FORMATS),renderer=ALL_TOOLS.find(t=>t.name==='render');
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
const text=(s,props='')=>`<Text font="Noto Sans SC" size={12} lineHeight={16} color="#292E33" ${props}>${esc(s)}</Text>`;
const folder=`<svg name="Folder artwork / same neutral Windows asset" w={44} h={44} viewBox="0 0 48 48"><path d="M5 13a3 3 0 0 1 3-3h10l4 5h18a3 3 0 0 1 3 3v19a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3Z" fill="#DFE5DF" stroke="#84958B" strokeWidth={1.2}/><path d="M5 19h38" fill="none" stroke="#A8B7AD" strokeWidth={1}/></svg>`;
const paper=`<svg name="Document artwork" w={44} h={44} viewBox="0 0 48 48"><path d="M12 5h16l10 10v26a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z" fill="#FAFBFC" stroke="#A6AEB4" strokeWidth={1.2}/><path d="M28 5v10h10M17 23h14M17 28h14M17 33h9" fill="none" stroke="#A6AEB4" strokeWidth={1.2}/></svg>`;
function tile(name,{selected=false,keyboard=false,inactive=false,file=false,x,y}={}){
 const label=name.length>11?'项目研究记录_最终…':name;
 return `<Frame name="File / ${esc(name)} · ${selected?'selected':'normal'}" ${x!==undefined?`x={${x}} y={${y}}`:''} w={96} h={100} flex="col" items="center" gap={4} py={2} px={6}>
 <Frame name="Icon selection plate / 56px" w={56} h={56} flex="row" items="center" justify="center" ${selected?'bg="#E7E9ED" rounded={6}':''}>${file?paper:folder}</Frame>
 <Frame name="Name slot / no full-width background" w="fill" h={36} flex="row" items="start" justify="center">
 <Frame name="Name highlight / hugs text" w="hug" h="hug" maxW={84} flex="col" items="center" px={4} py={2} rounded={4} ${selected?`bg="${inactive?'#D6DADF':'#0967D2'}"`:''} ${keyboard?'stroke="#63A5F2" strokeWidth={2} strokeAlign="outside"':''}>
 ${text(label,`${name.length>11?'w={76} maxLines={2} truncate={true}':''} textAlign="center" ${selected?`color="${inactive?'#292E33':'#FFFFFF'}"`:''}`)}
 </Frame></Frame></Frame>`;
}
function header(title='文档'){
 const control=(name,path)=>`<Frame name="${name}" w={32} h={44} flex="row" items="center" justify="center"><svg w={18} h={18} viewBox="0 0 18 18"><path d="${path}" fill="none" stroke="#606970" strokeWidth={1.3}/></svg></Frame>`;
 return `<Frame name="Custom title bar / retained v6 44px" w="fill" h={44} flex="row" items="center" bg="#F7F9F9" pl={8}>
 ${control('Back / Alt+Left','m11 4-5 5 5 5')}${control('Parent / Alt+Up','M9 15V3M4 8l5-5 5 5')}
 <Frame name="Location" h={32} px={8} flex="row" items="center">${text(title,'size={13}')}</Frame><Frame name="Native drag region" grow={1} h={44}/>
 ${['min','max','close'].map((s,i)=>`<Frame name="Native caption / ${s}" w={46} h={44} flex="row" items="center" justify="center"><svg w={14} h={14} viewBox="0 0 18 18"><path d="${['M4 9h10','M5 5h8v8H5Z','m5 5 8 8M13 5l-8 8'][i]}" fill="none" stroke="#606970" strokeWidth={1}/></svg></Frame>`).join('')}</Frame>`;
}
const names=['文档','图片','设计资料','研究笔记.pdf','项目研究记录_最终核对稿.pdf','预算.xlsx','归档','收据.pdf','旅行计划','个人资料','合同.pdf','参考资料'];
function screen(name,{marquee=false}={}){
 return `<Frame name="${name}" w={1120} h={720} flex="col" bg="#FFFFFF" rounded={8} overflow="hidden">${header()}
 <Frame name="Directory / unchanged 16px vertical 12px horizontal inset" w="fill" h="fill" grow={1}>
 ${names.map((s,i)=>tile(s,{x:12+i%11*100,y:16+Math.floor(i/11)*116,selected:marquee?[1,2,3].includes(i):i===0,file:s.includes('.')})).join('')}
 ${marquee?'<Rectangle name="Rubber band / no file-card outline" x={106} y={8} w={294} h={136} bg="#0967D21F" stroke="#3D86DE" strokeWidth={1}/>':''}
 </Frame><Frame name="Selection count / accessible status" w="fill" h={28} flex="row" items="center" px={12}>${text(marquee?'已选 3 项':'已选 1 项','size={11} color="#676D73"')}</Frame></Frame>`;
}
const screens={
 '45-selection-home':screen('45 · v7 选中态 / 图标底板与贴合名称'),
 '46-selection-states':`<Frame name="46 · v7 选择状态 / 鼠标、键盘、失焦与长名称" w={760} h={312} flex="col" gap={24} px={24} py={32} bg="#FFFFFF">
 ${text('选择与焦点','size={16} lineHeight={22}')}
 <Frame name="State examples" w="fill" flex="row" gap={56}>
 ${[{title:'鼠标选中',opts:{}},{title:'键盘焦点',opts:{keyboard:true}},{title:'窗口失焦',opts:{inactive:true}},{title:'长名称',opts:{file:true}}].map((s,i)=>`<Frame name="${s.title}" w={120} flex="col" gap={16} items="center">${tile(i===3?'项目研究记录_最终核对稿.pdf':'文档',{selected:true,...s.opts})}${text(s.title,'color="#676D73"')}</Frame>`).join('')}
 </Frame>${text('Ctrl / Shift 扩展选择 · 空格切换焦点项 · Esc 取消框选','size={11} color="#676D73"')}
 </Frame>`,
 '47-marquee':screen('47 · v7 鼠标框选 / 连续滚动与可见选区',{marquee:true})
};

await mkdir(out,{recursive:true});
const path=resolve(root,'design/Backleaf-v7.fig'),manifestPath=resolve(out,'manifest.json');
if(process.argv.includes('--verify')){
 const bytes=new Uint8Array(await readFile(path)),{graph}=await io.readDocument({name:path,data:bytes});populateAllFigPages(graph);
 const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
 if(graph.getPages().length!==47)throw Error('Expected retained 44 pages plus 3 selection pages');
 const codec=createCanvasKitRasterCodec(await initCanvasKit()),results=[];
 for(const expected of manifest.added){
  const page=graph.getPages().find(p=>p.name===expected.name),rootNode=graph.getChildren(page.id)[0];
  if(graph.countDescendants(rootNode.id)+1!==expected.nodes)throw Error('Lost editable nodes');
  const image=await headlessRenderNodes(graph,page.id,[rootNode.id],{format:'PNG',scale:1});
  const diff=comparePNGs(codec,new Uint8Array(await readFile(resolve(out,expected.key+'.png'))),image,{threshold:0.05});
  if(!diff||diff.changedPixels/diff.totalPixels>0.001)throw Error('Reopened render mismatch');
  results.push({name:expected.name,nodes:expected.nodes,changedPixels:diff.changedPixels,totalPixels:diff.totalPixels});
 }
 await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:44,added:results,result:'passed',verifiedAt:new Date().toISOString()},null,2));
 console.log(JSON.stringify({stage:'verified',pages:47,added:results}));process.exit(0);
}
const original=resolve(root,'design/Backleaf.fig'),bytes=new Uint8Array(await readFile(original));
if(createHash('sha256').update(bytes).digest('hex')!=='505c56ce3c827f128bc75bbc85cf131826acf56a3f5614e67b5b86feb47c3d4a')throw Error('v6 baseline changed; inspect before rebuilding');
const {graph}=await io.readDocument({name:original,data:bytes});populateAllFigPages(graph);
const figma=new FigmaAPI(graph);await measureDocumentText(graph);
const added=[];
for(const [key,jsx] of Object.entries(screens)){
 const page=figma.createPage();figma.currentPage=page;await renderer.loadFonts(figma,{jsx});
 const result=await editDocument(graph,()=>renderer.execute(figma,{jsx}));if(result?.error)throw Error(JSON.stringify(result));
 const node=graph.getChildren(page.id)[0];page.name=node.name;
 const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});await writeFile(resolve(out,key+'.png'),image);
 added.push({key,name:page.name,pageId:page.id,rootId:node.id,nodes:graph.countDescendants(node.id)+1});
 console.log(JSON.stringify(added.at(-1)));
}
figma.currentPage=figma.root.children.find(p=>p.id===added[0].pageId);
const written=await io.writeDocument('fig',graph,{renderThumbnail:true});await writeFile(path,written.data);
await writeFile(manifestPath,JSON.stringify({version:'v7-selection',baseline:'Backleaf.fig v6',retainedPages:44,added,source:'scripts/design/selection-v7.mjs',rule:'New pages supersede selection/focus examples in retained v6 states.'},null,2));
await writeFile(resolve(out,'source.json'),JSON.stringify(screens,null,2));console.log(JSON.stringify({saved:path,pages:47}));process.exit(0);
