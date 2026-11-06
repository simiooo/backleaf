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

const root=resolve(import.meta.dir,'../..');
const base=resolve(root,'design/Backleaf-v46.fig');
const file=resolve(root,'design/Backleaf-v47.fig');
const out=resolve(root,'design/v47');
const io=new IORegistry(BUILTIN_IO_FORMATS);
const verify=process.argv.includes('--verify');
const exportOnly=process.argv.includes('--export');
await mkdir(out,{recursive:true});
await measureTextHeadlessly();
const bytes=new Uint8Array(await readFile((verify||exportOnly)?file:base));
const {graph}=await io.readDocument({name:file,data:bytes});
populateAllFigPages(graph);
const spec=JSON.parse(await readFile(resolve(root,'design/color-system.json'),'utf8'));
const elevation=JSON.parse(await readFile(resolve(root,'design/elevation-system.json'),'utf8'));
const figma=new FigmaAPI(graph);
const collection=figma.variables.getLocalVariableCollections().find(c=>c.name==='Backleaf colors');
const modes=Object.fromEntries(collection.modes.map(m=>[m.name,m.modeId]));
const rgba=h=>{const x=h.replace('#','');return {r:parseInt(x.slice(0,2),16)/255,g:parseInt(x.slice(2,4),16)/255,b:parseInt(x.slice(4,6),16)/255,a:x.length===8?parseInt(x.slice(6,8),16)/255:1};};
const hexOf=c=>'#'+[c.r,c.g,c.b].map(v=>Math.round(v*255).toString(16).padStart(2,'0')).join('').toUpperCase();
const targetNames=['01 · Home / 紧凑中性','03 · 统一快捷访问','04 · 多选与上下文菜单','08 · 深色模式','09 · 多文件详情','10 · 失败集中呈现'];
const targetPages=targetNames.map((name)=>({name,page:graph.getPages().find(p=>p.name===name)})).filter(x=>x.page);
if(targetPages.length!==targetNames.length)throw Error('Missing target pages: '+targetNames.filter((n,i)=>!targetPages[i]).join(','));

async function renderPage(page,key){
  const node=graph.getChildren(page.id)[0];
  const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});
  if(!image)throw Error('No render '+key);
  await writeFile(resolve(out,key+'.png'),image);
  return {key,page:page.name,nodes:graph.countDescendants(node.id)+1};
}
if(exportOnly){
  const manifest=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8'));
  for(const expected of manifest.pages){
    const pageName=expected.name||expected.page;
    const page=graph.getPages().find(p=>p.name===pageName);
    if(!page)throw Error('Page missing '+pageName);
    const node=graph.getChildren(page.id)[0];
    await writeFile(resolve(out,expected.key+'.png'),await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1}));
  }
  await writeFile(resolve(out,'exported.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),pages:manifest.pages.length,exportedAt:new Date().toISOString()},null,2));
  console.log(JSON.stringify({exported:manifest.pages.length}));
  process.exit(0);
}
if(verify){
  const manifest=JSON.parse(await readFile(resolve(out,'manifest.json'),'utf8'));
  const codec=createCanvasKitRasterCodec(await initCanvasKit());
  for(const expected of manifest.pages){
    const pageName=expected.name||expected.page;
    const page=graph.getPages().find(p=>p.name===pageName);
    if(!page)throw Error('Page missing '+pageName);
    const node=graph.getChildren(page.id)[0];
    if(graph.countDescendants(node.id)+1!==expected.nodes)throw Error('Page nodes changed '+pageName);
    const image=await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1});
    const old=new Uint8Array(await readFile(resolve(out,expected.key+'.png')));
    const diff=comparePNGs(codec,old,image,{threshold:.05});
    if(!diff||diff.changedPixels/diff.totalPixels>.001)throw Error('Cold render changed '+expected.key);
    expected.changedPixels=diff.changedPixels;expected.totalPixels=diff.totalPixels;
  }
  const retained=graph.getPages().slice(0,manifest.retainedPages).map(p=>({name:p.name,nodes:graph.countDescendants(p.id)+1}));
  if(JSON.stringify(retained)!==JSON.stringify(manifest.retained))throw Error('Retained pages changed');
  await writeFile(resolve(out,'cold-reopen.json'),JSON.stringify({masterSha256:createHash('sha256').update(bytes).digest('hex'),retainedPages:manifest.retainedPages,pages:manifest.pages.length,result:'passed',pagesChecked:manifest.pages,verifiedAt:new Date().toISOString()},null,2));
  console.log(JSON.stringify({verified:true,pages:graph.getPages().length}));
  process.exit(0);
}

// 1. Render a small before/after sample of the existing master.
for(const {name,page} of targetPages){
  const key='before-'+(name.split(' ·')[0].trim());
  await renderPage(page,key);
}
// 2. Recolor theme colors across all existing pages. This is the color-only
//    phase: geometry, text, shadows and image assets remain untouched.
const map={
  // light semantic + neutral support shades
  '#1C2B33':'#1A1B1E','#596975':'#5E6065','#98A4AE':'#9CA0A6','#D7DEE5':'#E4E4E6',
  '#E9F2FC':'#EAEBEE','#17466F':'#1A1B1E','#EDF0F3':'#F1F1F3','#334450':'#5E6065',
  '#F3F5F7':'#F2F2F3','#BBD0E1':'#E4E6E8','#58758D':'#83878D','#FAFCFE':'#FFFFFF',
  '#A9B6C1':'#B8BCC2','#F6F8FA':'#F5F5F6','#F8F9FA':'#F7F7F8','#DCE2E7':'#E6E6E8',
  '#AEB7BF':'#A8ACB2','#E3E7EA':'#E9EAEC','#D4DADE':'#D9DADD','#E5E8EB':'#E9EAEB',
  '#DDE1E5':'#E2E3E5','#F0F2F4':'#F3F3F4','#F6F7F8':'#F7F7F8','#CDD3D8':'#D4D5D8',
  '#F4F6F7':'#F6F6F7','#FAFBFC':'#FCFCFD','#B2BAC1':'#B4B7BC','#BBC3CA':'#BCC0C5',
  '#87929B':'#8A8E94','#676D73':'#62666C','#70777E':'#6B6F75','#697E8D':'#7A7E84',
  '#697781':'#7A7E84','#73818A':'#74787E','#8B99A4':'#8C9096','#8F9AA3':'#92969C',
  '#7D8E9B':'#878B91','#829BAB':'#A7ABB1','#A8BFCE':'#CFD2D6','#C9D7E0':'#E0E2E5',
  '#9ABADA':'#C8CBCF','#B5C5D9':'#D2D4D8','#EDF0F2':'#F0F1F3',
  // dark semantic + neutral support shades
  '#161D25':'#0D0E10','#1C252F':'#151618','#212B35':'#1C1D21','#27323E':'#26272B',
  '#344454':'#2D2E32','#E8EEF4':'#F2F3F5','#AAB8C5':'#A6A8AD','#71818F':'#6C6F75',
  '#485766':'#2D2E32','#203C58':'#2D2F34','#E0EEFF':'#F2F3F5','#2B3743':'#232427',
  '#E0E8EF':'#D5D7DB','#95C2FF':'#8AB4FF','#799BB8':'#3B3D42','#BAD0E3':'#A9ACB2',
  '#9BAAB8':'#9CA0A7','#292E33':'#1D1E21','#2E3439':'#222327','#424A51':'#2B2C30',
  '#20262C':'#151619','#30373D':'#242529','#161A1E':'#101113','#202326':'#141517',
  '#102B4A':'#0B1B33','#647786':'#747980','#687986':'#787C82','#485F70':'#5F6369',
  '#31566F':'#5A5E64','#0966DE':'#0866DF'
};
const mapped=new Map();
for(const n of graph.getAllNodes()){
  const api=figma.getNodeById(n.id);
  for(const field of ['fills','strokes']){
    const arr=n[field];if(!arr)continue;
    let changed=false;
    const next=arr.map(p=>{
      if(p.type!=='SOLID')return p;
      const h=hexOf(p.color);const to=map[h];if(!to)return p;
      const c=rgba(to);changed=true;mapped.set(h,(mapped.get(h)||0)+1);
      return {type:'SOLID',visible:p.visible,opacity:p.opacity,blendMode:p.blendMode,color:{r:c.r,g:c.g,b:c.b,a:p.color.a??1}};
    });
    if(changed)api[field]=next;
  }
}
// Keep the variable collection in sync for newly generated pages.
for(const [modeName,modeValues] of Object.entries(spec.modes)){
  for(const [role,hex] of Object.entries(modeValues)){
    let variable=figma.variables.getLocalVariables().find(v=>v.name===role&&v.variableCollectionId===collection.id);
    if(!variable)variable=figma.variables.createVariable(role,collection.id,'COLOR');
    variable.setValueForMode(modes[modeName],rgba(hex));
  }
}
// 3. Render the recolored master sample.
const pages=[];
for(const {name,page} of targetPages){
  const key='after-'+(name.split(' ·')[0].trim());
  pages.push(await renderPage(page,key));
}

// 4. Add the v47 floating/elevation study pages.
const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w,role,size=13,line=20,weight=400)=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${line}} weight={${weight}} color=${C(role)} w={${w}} h={${line}}>${s}</Text>`;
const svgFolder=(dark)=>`<svg w={44} h={44} viewBox="0 0 48 48"><path d="M5 13Q5 9 9 9H18Q20 9 23 13H39Q43 13 43 18V35Q43 39 39 39H9Q5 39 5 35Z" fill="${dark?'#3B3D42':'#E4E6E8'}" stroke="${dark?'#A9ACB2':'#83878D'}" strokeWidth={1.2}/><path d="M5 19H43" fill="none" stroke="${dark?'#A9ACB2':'#83878D'}" strokeWidth={1}/></svg>`;
const svgFile=(dark)=>`<svg w={44} h={44} viewBox="0 0 48 48"><path d="M12 5H28L36 13V43H12Z" fill="${dark?'#26272A':'#FFFFFF'}" stroke="${dark?'#9CA0A7':'#B8BCC2'}" strokeWidth={1.2}/><path d="M28 5V13H36M17 22H31M17 27H31M17 32H27" fill="none" stroke="${dark?'#9CA0A7':'#B8BCC2'}" strokeWidth={1.2}/></svg>`;
const boardShadow=dark=>({z1:elevation.levels.z1.dark,z2:elevation.levels.z2.dark,z3:elevation.levels.z3.dark,halo:'0 0 0 4 #8AB4FF33, 0 8 24 #00000066'})||{};
function elevationBoard(dark){
  const s=dark?{z1:elevation.levels.z1.dark,z2:elevation.levels.z2.dark,z3:elevation.levels.z3.dark,halo:'0 0 0 4 #8AB4FF33, 0 8 24 #00000066'}:{z1:elevation.levels.z1.light,z2:elevation.levels.z2.light,z3:elevation.levels.z3.light,halo:'0 0 0 4 #0866DF26, 0 8 24 #1A1B1E1A'};
  const card=(name,height,bg,shadow)=>
    F(name,
      T(name,196,'content/primary',14,20,500)+
      T('柔和阴影 + 中性表面；没有描边、边框或蓝色填充。',196,'content/secondary',12,18),
      `w={240} h={${height}} p={16} rounded={16} bg=${C(bg)} flex="col" gap={6} shadow="${shadow}"`);
  const state=(name,shadow,borderless=true)=>
    F(name,
      T(name,196,'content/primary',14,20,500)+
      T('抬升一层/阴影变化 + 保持不变的文字与图标',196,'content/secondary',11,17),
      `w={240} h={148} p={16} rounded={16} bg=${C('surface/floating')} flex="col" gap={6} shadow="${shadow}"`);
  return F('v47 elevation board',
    T('Backleaf v47 · 中性色与悬浮层级',1024,'content/primary',24,32,500)+
    T('换色后，层级和重点只由表面明度与阴影高度表达；不再用 outline、边框或饱和色块来区分。',1024,'content/secondary',13,20)+
    F('z levels',card('z0 · canvas',120,'surface/canvas','none')+card('z1 · chrome',180,'surface/chrome',s.z1)+card('z2 · file surface',260,'surface/files',s.z2)+card('z3 · floating',340,'surface/floating',s.z3),
      'w="fill" h={372} flex="row" items="end" gap={16}')+
    F('states',state('normal','none')+state('hover',s.z1)+state('selected',s.z3)+state('focus',s.halo),
      'w="fill" h={168} flex="row" gap={16}'),
    `w={1120} h={760} p={32} bg=${C('surface/canvas')} flex="col" gap={16}`);
}
function gridStudy(dark){
  const s=dark?{z2:elevation.levels.z2.dark,z3:elevation.levels.z3.dark}:{z2:elevation.levels.z2.light,z3:elevation.levels.z3.light};
  const row=(names,selected=-1)=>F('row',
    names.map((name,i)=>F('item '+name,
      (name.startsWith('文件夹')?svgFolder(dark):svgFile(dark))+
      T(name,132,'content/primary',12,18),
      `w={144} h={112} flex="col" items="center" gap={6} p={8} rounded={14} ${i===selected?`bg=${C('surface/floating')} shadow="${s.z3}"`:i===selected+1?`shadow="${s.z1}"`:''}`)).join(''),
    'w="fill" flex="row" gap={16}');
  const sidebarItem=(name,active)=>F('nav '+name,
    F('dot','', `w={18} h={18} rounded={5} bg=${C('surface/input-rest')}`)+T(name,160,'content/primary',13,20,active?500:400),
    `w="fill" h={36} px={8} flex="row" items="center" gap={8} rounded={10} ${active?`bg=${C('surface/floating')} shadow="${s.z3}"`:''}`);
  return F('v47 grid study',
    F('sidebar',T('快捷访问',200,'content/secondary',12,18)+sidebarItem('个人文件夹',false)+sidebarItem('桌面',false)+sidebarItem('下载',true)+sidebarItem('文档',false)+sidebarItem('图片',false),
      `x={0} y={0} w={240} h={720} p={16} bg=${C('surface/chrome')} flex="col" gap={4}`)+
    F('main surface',T('D:\\work\\backleaf',760,'content/secondary',13,20)+row(['文件夹 项目资料','文件夹 设计稿','会议记录.docx','预算.xlsx'])+row(['文件夹 参考资料','说明.pdf','app.tsx','README.md'],2),
      `x={260} y={24} w={836} h={672} p={24} rounded={18} bg=${C('surface/files')} flex="col" gap={16} shadow="${s.z2}"`)+
    F('floating task',
      T('正在复制 4 项',300,'content/primary',13,20,500)+T('2.4 MB / 8.1 MB · 安静进行',300,'content/secondary',12,18)+
      F('track',F('fill','',`w={188} h={4} rounded={2} bg=${C('accent/strong')}`),`w="fill" h={4} rounded={2} bg=${C('surface/input-rest')}`),
      `x={700} y={560} w={340} h={120} p={16} rounded={16} bg=${C('surface/floating')} flex="col" gap={8} shadow="${s.z3}"`),
    `w={1120} h={720} bg=${C('surface/canvas')}`);
}
const render=ALL_TOOLS.find(t=>t.name==='render');
const studies=[['272-v47-elevation-light','272 · v47 悬浮层级 / 浅色',elevationBoard(false),'Light'],['273-v47-elevation-dark','273 · v47 悬浮层级 / 深色',elevationBoard(true),'Dark'],['274-v47-grid-selection-light','274 · v47 网格选中悬浮 / 浅色',gridStudy(false),'Light'],['275-v47-grid-selection-dark','275 · v47 网格选中悬浮 / 深色',gridStudy(true),'Dark']];
for(const [key,name,jsx,mode] of studies){
  const page=figma.createPage();figma.currentPage=page;page.name=name;
  await render.loadFonts(figma,{jsx});
  const result=await editDocument(graph,()=>render.execute(figma,{jsx}));
  if(result?.error)throw Error(JSON.stringify(result));
  const node=graph.getChildren(page.id)[0];figma.getNodeById(node.id).setExplicitVariableModeForCollection(collection.id,modes[mode]);
  await writeFile(resolve(out,key+'.png'),await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1}));
  pages.push({key,name,mode,nodes:graph.countDescendants(node.id)+1});
  console.log(JSON.stringify({stage:'study',key,nodes:pages.at(-1).nodes}));
}
figma.currentPage=figma.root.children.find(p=>p.name==='01 · Home / 紧凑中性');
await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);
const retained=graph.getPages().slice(0,271).map(p=>({name:p.name,nodes:graph.countDescendants(p.id)+1}));
await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v47-neutral-elevation-draft',retainedPages:271,pages,retained,recoloredPaints:mapped.size,colorMap:Object.fromEntries([...mapped.entries()].sort((a,b)=>b[1]-a[1]).slice(0,120)),source:'scripts/design/theme-v47.mjs'},null,2));
console.log(JSON.stringify({saved:file,pages:graph.getPages().length,recolored:mapped.size}));
