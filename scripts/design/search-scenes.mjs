import {scenes as navigationScenes} from './navigation-scenes.mjs';
export function scenes(api){
 const {F,T,C,spacer}=api;
 const searchIcon=`<svg w={18} h={18} viewBox="0 0 24 24"><path d="M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15m5.5 13L22 22" fill="none" stroke="#6A7772" strokeWidth={1.5}/></svg>`;
 const entry=F('Search entry / Ctrl+F',searchIcon+T('搜索',{role:'content/secondary'})+T('Ctrl+F',{size:11,role:'content/secondary'}),'w={112} h={32} px={8} flex="row" items="center" gap={8} rounded={8}');
 return ['entry','input','running','results','empty','incomplete','narrow','dark'].map((state,i)=>{
  const narrow=state==='narrow',width=narrow?720:1120,panelW=narrow?480:560;
  let jsx=navigationScenes(api)[narrow?7:0].jsx;
  jsx=jsx.replace('<Frame name="Caption"',entry+'<Frame name="Caption"');
  if(state!=='entry'){
   const input=F('Search field / native IME',T(['input','empty'].includes(state)?'搜索文件名': '报告',{role:state==='input'?'content/secondary':'content/primary',w:panelW-100,maxLines:1,truncate:true}),'w="fill" h={32} px={8} flex="row" items="center" gap={8} rounded={8} stroke='+C('focus/ring')+' strokeWidth={2}');
   const scope=T('文档 · 含子文件夹',{size:12,role:'content/secondary',w:panelW-24,maxLines:1,truncate:true});
   const results=['研究报告_最终核对稿与补充资料_2026年11月.pdf','报告.xlsx','项目报告.pdf','报告归档'].map((n,j)=>F('Result / show in folder',F('Result labels',T(n,{w:panelW-64,maxLines:1,truncate:true})+T(j===2?'项目资料 / 研究 / 参考文献':'合同 / 项目甲',{size:12,role:'content/secondary',w:panelW-64,maxLines:1,truncate:true}),'w="fill" flex="col"')+T('↗',{role:'content/secondary'}),'w="fill" h={48} px={8} flex="row" items="center" gap={8} rounded={8} '+(j===0?'bg='+C('interaction/hover'):''))).join('');
   const status=state==='running'?'搜索中 · 已检查 42,108 项':state==='empty'?'没有匹配项':state==='input'?'':state==='incomplete'?'4 项 · 2 个位置无法访问':'4 项';
   const content=F('Search popover / bounded virtual results',input+spacer(12)+scope+(state==='input'?'':spacer(16)+(state==='empty'?'':results)+spacer(12)+T(status,{size:12,role:state==='incomplete'?'status/negative':'content/secondary'})),`position="absolute" x={${width-panelW-12}} y={48} w={${panelW}} h="hug" px={12} py={16} flex="col" rounded={12} bg=${C('surface/floating')} stroke=${C('border/subtle')} strokeWidth={1} shadow="0 8 24 #14232B1F"`);
   jsx=jsx.slice(0,-8)+content+'</Frame>';
  }
  const key=String(85+i)+'-search-'+state,name=String(85+i)+' · v12 搜索 '+state;
  return {key,name,mode:state==='dark'?'Dark':'Light',jsx};
 });
}
