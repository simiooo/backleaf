import {readFile,writeFile,mkdir,cp} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes,initCanvasKit,createCanvasKitRasterCodec,comparePNGs} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureTextHeadlessly,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';

// v48: Apple-inspired surfaces, blue folder identity, raised identity selection
// with blue filename, corrected canonical geometry.
const root=resolve(import.meta.dir,'../..');
const base=resolve(root,'design/Backleaf-v46.fig');
const file=resolve(root,'design/Backleaf-v48.fig');
const out=resolve(root,'design/v48');
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

const oldToV47={
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
  '#161D25':'#0D0E10','#1C252F':'#151618','#212B35':'#1C1D21','#27323E':'#26272B',
  '#344454':'#2D2E32','#E8EEF4':'#F2F3F5','#AAB8C5':'#A6A8AD','#71818F':'#6C6F75',
  '#485766':'#2D2E32','#203C58':'#2D2F34','#E0EEFF':'#F2F3F5','#2B3743':'#232427',
  '#E0E8EF':'#D5D7DB','#95C2FF':'#8AB4FF','#799BB8':'#3B3D42','#BAD0E3':'#A9ACB2',
  '#9BAAB8':'#9CA0A7','#292E33':'#1D1E21','#2E3439':'#222327','#424A51':'#2B2C30',
  '#20262C':'#151619','#30373D':'#242529','#161A1E':'#101113','#202326':'#141517',
  '#102B4A':'#0B1B33','#647786':'#747980','#687986':'#787C82','#485F70':'#5F6369',
  '#31566F':'#5A5E64','#0966DE':'#0866DF'
};
const v47ToV48={
  // light Apple neutrals and blue identity
  '#1A1B1E':'#1D1D1F','#5E6065':'#6E6E73','#8A8E94':'#8E8E93','#F7F7F8':'#F2F2F7',
  '#E6E6E8':'#D2D2D7','#A8ACB2':'#AEAEB2','#B8BCC2':'#C7C7CC','#F2F3F5':'#F2F2F7',
  '#F5F5F6':'#F2F2F7','#62666C':'#6E6E73','#EAEBEE':'#E8F2FF','#E4E6E8':'#BBD0E1',
  '#E4E4E6':'#D2D2D7','#F4F4F5':'#FFFFFF','#E9EAEC':'#E5E5EA','#F2F2F3':'#F2F2F7',
  '#A7ABB1':'#9FBEDD','#CFD2D6':'#C9DDF1','#E0E2E5':'#DCE9F7','#7A7E84':'#58758D',
  '#D9DADD':'#D2D2D7','#E9EAEB':'#E5E5EA','#74787E':'#8E8E93','#E2E3E5':'#E5E5EA',
  '#A9ACB2':'#B6D3F1','#6B6F75':'#6E6E73','#D2D4D8':'#C7C7CC','#BCC0C5':'#C7C7CC',
  '#9CA0A6':'#AEAEB2','#F3F3F4':'#F2F2F7','#B4B7BC':'#C7C7CC','#D4D5D8':'#D2D2D7',
  '#F6F6F7':'#F2F2F7','#8C9096':'#8E8E93','#C8CBCF':'#C9DDF1','#FCFCFD':'#FFFFFF',
  '#F0F1F3':'#F2F2F7','#D3D9DE':'#D2D2D7','#878B91':'#8E8E93','#92969C':'#8E8E93',
  '#F1F1F3':'#F2F2F7','#D5D7DB':'#A1A1A6',
  // dark Apple neutrals and blue identity
  '#1D1E21':'#38383A','#83878D':'#58758D','#26272B':'#2C2C2E','#151618':'#1C1C1E',
  '#2D2F34':'#14324D','#2D2E32':'#38383A','#0D0E10':'#000000','#3B3D42':'#5277A3',
  '#9CA0A7':'#8E8E93','#A6A8AD':'#A1A1A6','#8AB4FF':'#0A84FF','#1C1D21':'#000000',
  '#232427':'#1C1C1E','#222327':'#1C1C1E','#26272A':'#2C2C2E','#292D31':'#1C1C1E',
  '#2B2C30':'#38383A','#747980':'#8E8E93','#787C82':'#8E8E93','#5F6369':'#6E6E73',
  '#151619':'#1C1C1E','#242529':'#1C1C1E','#6C6F75':'#6E6E73','#101113':'#000000',
  '#141517':'#1C1C1E','#3C444B':'#38383A','#5A5E64':'#6E6E73','#0B1B33':'#0A1A2B',
  '#0866DF':'#0866DF','#000000':'#000000','#FFFFFF':'#FFFFFF'
};
const mapped=new Map();
for(const n of graph.getAllNodes()){
  const api=figma.getNodeById(n.id);
  for(const field of ['fills','strokes']){
    const arr=n[field];if(!arr)continue;
    let changed=false;
    const next=arr.map(p=>{
      if(p.type!=='SOLID')return p;
      const h=hexOf(p.color);
      const v47=oldToV47[h];
      const target=v47?(v47ToV48[v47]||v47):v47ToV48[h];
      if(!target||target===h)return p;
      const c=rgba(target);changed=true;mapped.set(h,(mapped.get(h)||0)+1);
      return {type:'SOLID',visible:p.visible,opacity:p.opacity,blendMode:p.blendMode,color:{r:c.r,g:c.g,b:c.b,a:p.color.a??1}};
    });
    if(changed)api[field]=next;
  }
}
for(const [modeName,modeValues] of Object.entries(spec.modes)){
  for(const [role,value] of Object.entries(modeValues)){
    let variable=figma.variables.getLocalVariables().find(v=>v.name===role&&v.variableCollectionId===collection.id);
    if(!variable)variable=figma.variables.createVariable(role,collection.id,'COLOR');
    variable.setValueForMode(modes[modeName],rgba(value));
  }
}

const C=r=>`{designVar('Backleaf colors','${r}')}`;
const F=(n,b,p='')=>`<Frame name="${n}" ${p}>${b}</Frame>`;
const T=(s,w,role,size=13,line=20,weight=400)=>`<Text font="Noto Sans SC" size={${size}} lineHeight={${line}} weight={${weight}} color=${C(role)} w={${w}} h={${line}}>${s}</Text>`;
const R={control:8,identity:10,floating:12,row:8,panel:12,surface:16};
const S={contentX:12,contentY:16,section:16,inline:8,collection:8,identity:6,panelX:12,panelY:16};
const lightColors={folderFill:'#BBD0E1',folderStroke:'#58758D',paper:'#FFFFFF',contour:'#C7C7CC',text:'#1D1D1F',sub:'#6E6E73'};
const darkColors={folderFill:'#5277A3',folderStroke:'#B6D3F1',paper:'#2C2C2E',contour:'#8E8E93',text:'#F5F5F7',sub:'#A1A1A6'};
const cat={word:'#376DA4',sheet:'#37795B',ppt:'#B36B3F',pdf:'#B44949',code:'#37795B',zip:'#99753C',img:'#367E83',generic:'#64768C'};
const catDark={word:'#A1C6EB',sheet:'#9ACDB3',ppt:'#E5B28F',pdf:'#EAB1AE',code:'#9ACDB3',zip:'#DAC396',img:'#9CCDD0',generic:'#B5C5D9'};
const folderSvg=(dark,size=44)=>`<svg w={${size}} h={${size}} viewBox="0 0 48 48"><path d="M5 13Q5 9 9 9H18Q20 9 23 13H39Q43 13 43 18V35Q43 39 39 39H9Q5 39 5 35Z" fill="${dark?darkColors.folderFill:lightColors.folderFill}" stroke="${dark?darkColors.folderStroke:lightColors.folderStroke}" strokeWidth={1.2}/><path d="M5 19H43" fill="none" stroke="${dark?darkColors.folderStroke:lightColors.folderStroke}" strokeWidth={1}/></svg>`;
const fileSvg=(kind,dark)=>{const color=(dark?catDark:cat)[kind];const p=dark?darkColors.paper:lightColors.paper;const outline=dark?darkColors.contour:lightColors.contour;return `<svg w={44} h={44} viewBox="0 0 48 48"><path d="M12 5H28L36 13V43H12Z" fill="${p}" stroke="${outline}" strokeWidth={1.1}/><path d="M28 5V13H36" fill="none" stroke="${outline}" strokeWidth={1.1}/><rect x={16} y={21} width={16} height={3} rx={1} fill="${color}"/><rect x={16} y={27} width={13} height={2} rx={1} fill="${color}" opacity={0.7}/><rect x={16} y={32} width={9} height={2} rx={1} fill="${color}" opacity={0.45}/></svg>`;};
const rowShadow=dark=>dark?elevation.levels.z3.dark:elevation.levels.z3.light;
const surfaceShadow=dark=>dark?elevation.levels.z2.dark:elevation.levels.z2.light;
function board(dark){
  const bg='surface/canvas';
  const cards=[
    ['z0 · content','surface/files',dark?elevation.levels.z2.dark:elevation.levels.z2.light,240,'白色内容画布；文件夹蓝、文件类型保留识别色。'],
    ['z1 · chrome','surface/chrome','none',240,'侧栏/标题栏使用中性浅灰，不延伸到内容面。'],
    ['z2 · floating identity','surface/floating',rowShadow(dark),240,'选中只抬起 identity 面，文件名转蓝，无描边。'],
    ['z3 · menu / task','surface/floating',rowShadow(dark),240,'菜单/任务面板浮起；正文保持白/黑。']
  ];
  const card=(name,bg,shadow,h,desc)=>F(name,
    T(name,196,'content/primary',14,20,500)+T(desc,196,'content/secondary',12,18)+
    F('sample identity',
      folderSvg(dark)+T('项目资料',96,dark?'accent/strong':'accent/strong',12,18),
      `w={196} h={88} mt={8} p={8} rounded={10} bg=${C('surface/floating')} shadow="${shadow}" flex="row" items="center" gap={6}`),
    `w={226} h={${h}} p={12} rounded={12} bg=${C(bg)} flex="col" gap={8} shadow="${shadow}"`);
  const state=(name,role,desc)=>F(name,
    T(name,206,'content/primary',14,20,500)+T(desc,206,'content/secondary',11,17),
    `w={226} h={126} p={12} rounded={12} bg=${C(role)} flex="col" gap={8} shadow="${name==='selected'?rowShadow(dark):'none'}"`);
  return F('v48 Apple surfaces board',
    T('Backleaf v48 · 白/黑内容 + 蓝身份 + 悬浮层级',1024,'content/primary',24,32,500)+
    T('浅色内容白、深色内容黑；灰只用于 chrome；文件夹保留蓝色；文件类型保留识别色；选中抬起 identity 面并让文件名转蓝。',1024,'content/secondary',13,20)+
    F('surface levels',cards.map(c=>card(...c)).join(''),'w="fill" h={260} flex="row" items="end" gap={8}')+
    F('states',
      state('normal','surface/files','平面内容层；无阴影、无描边。')+
      state('hover','surface/floating','仅轻抬一层；文字/图标颜色不变。')+
      state('selected','surface/floating','抬起 identity 面 + 蓝色文件名；无整格填充。')+
      state('focus','surface/floating','柔和 accent halo，仅用于可访问性。'),
      'w="fill" h={150} flex="row" gap={8}'),
    `w={1120} h={760} p={16} bg=${C(bg)} flex="col" gap={16}`);
}
function appStudy(dark){
  const chrome=dark?darkColors:lightColors;
  const sidebarRow=(name,active)=>F('nav '+name,
    folderSvg(dark,18)+T(name,164,active?'accent/strong':'content/primary',13,20,active?500:400),
    `w="fill" h={32} px={8} flex="row" items="center" gap={6} rounded={8} ${active?`bg=${C('surface/floating')} shadow="${rowShadow(dark)}"`:''}`);
  const identity=(name,kind,selected=false)=>{
    const icon=kind==='folder'?folderSvg(dark):fileSvg(kind,dark);
    if(selected){
      return F('file identity / '+name,
        F('selected identity',
          icon+T(name,116,'accent/strong',12,16),
          `w={132} h={92} p={8} rounded={10} bg=${C('surface/floating')} shadow="${rowShadow(dark)}" flex="col" items="center" justify="center" gap={6}`),
        `w={192} h={112} flex="col" items="center" justify="center"`);
    }
    return F('file identity / '+name,
      icon+T(name,180,'content/primary',12,16),
      `w={192} h={112} flex="col" items="center" justify="center" gap={6}`);
  };
  const folder='folder';
  return F('v48 app study',
    F('titlebar',
      T('D:\\work\\backleaf',520,'content/secondary',13,20)+F('spacer','', 'grow={1}')+T('⌕   —   □   ×',160,'content/secondary',13,20),
      `x={0} y={0} w={1120} h={44} px={12} bg=${C('surface/chrome')} flex="row" items="center" gap={8}`)+
    F('sidebar',
      T('快捷访问',200,'content/secondary',12,18)+
      sidebarRow('个人文件夹',false)+sidebarRow('桌面',false)+sidebarRow('下载',true)+sidebarRow('文档',false)+sidebarRow('图片',false),
      `x={0} y={44} w={240} h={676} px={8} py={16} bg=${C('surface/chrome')} flex="col" gap={4}`)+
    F('file surface',
      T('D:\\work\\backleaf',820,'content/secondary',13,20)+
      F('grid row 1',identity('项目资料',folder)+identity('设计稿',folder)+identity('会议记录.docx','word',true)+identity('预算.xlsx','sheet'),'w="fill" flex="row" gap={8}')+
      F('grid row 2',identity('方案.pptx','ppt')+identity('说明.pdf','pdf')+identity('app.tsx','code')+identity('README.md','code'),'w="fill" flex="row" gap={8}'),
      `x={246} y={52} w={868} h={660} px={12} py={16} rounded={16} bg=${C('surface/files')} shadow="${surfaceShadow(dark)}" flex="col" gap={16}`)+
    F('task panel',
      T('正在复制 4 项',288,'content/primary',13,20,500)+T('2.4 MB / 8.1 MB · 安静进行',288,'content/secondary',12,18)+
      F('track',F('fill','', `w={172} h={4} rounded={2} bg=${C('accent/strong')}`),`w="fill" h={4} rounded={2} bg=${C('surface/input-rest')}`),
      `x={720} y={556} w={328} h={112} p={12} rounded={12} bg=${C('surface/floating')} shadow="${rowShadow(dark)}" flex="col" gap={8}`),
    `w={1120} h={720} bg=${C('surface/canvas')}`);
}
const render=ALL_TOOLS.find(t=>t.name==='render');
const studies=[
  ['272-v48-apple-light','272 · v48 Apple 表面 / 浅色',board(false),'Light'],
  ['273-v48-apple-dark','273 · v48 Apple 表面 / 深色',board(true),'Dark'],
  ['274-v48-app-study-light','274 · v48 选中与文件类型 / 浅色',appStudy(false),'Light'],
  ['275-v48-app-study-dark','275 · v48 选中与文件类型 / 深色',appStudy(true),'Dark']
];
const pages=[];
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
// Sample existing master pages after recolor.
const sampleNames=['04 · 多选与上下文菜单','08 · 深色模式'];
for(const name of sampleNames){
  const page=graph.getPages().find(p=>p.name===name);
  if(!page)continue;
  const key='after-'+(name.split(' ·')[0].trim());
  const node=graph.getChildren(page.id)[0];
  await writeFile(resolve(out,key+'.png'),await headlessRenderNodes(graph,page.id,[node.id],{format:'PNG',scale:1}));
  pages.push({key,page:name,nodes:graph.countDescendants(node.id)+1});
}
figma.currentPage=figma.root.children.find(p=>p.name==='01 · Home / 紧凑中性');
await writeFile(file,(await io.writeDocument('fig',graph,{renderThumbnail:true})).data);
const retained=graph.getPages().slice(0,271).map(p=>({name:p.name,nodes:graph.countDescendants(p.id)+1}));
await writeFile(resolve(out,'manifest.json'),JSON.stringify({version:'v48-apple-neutral-blue-elevation',masterSha256:createHash('sha256').update(await readFile(file)).digest('hex'),retainedPages:271,pages,retained,recoloredPaints:mapped.size,source:'scripts/design/theme-v48.mjs',colorSystemSha256:createHash('sha256').update(await readFile(resolve(root,'design/color-system.json'))).digest('hex'),elevationSystemSha256:createHash('sha256').update(await readFile(resolve(root,'design/elevation-system.json'))).digest('hex')},null,2));
console.log(JSON.stringify({saved:file,pages:graph.getPages().length,recolored:mapped.size}));
