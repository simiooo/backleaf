import {scenes as searchScenes} from './search-scenes.mjs';
import {scenes as navigationScenes} from './navigation-scenes.mjs';
export function scenes(api){
 const {F,T,C,spacer,tile,menu}=api;
 const field=(name,value,active=true,error=false)=>F(name,T(value,{w:280,maxLines:1,truncate:true}),'w={320} h={32} px={8} flex="row" items="center" rounded={8} bg='+C(active?'surface/input-active':'surface/input-rest')+' stroke='+C(error?'status/negative':'border/subtle')+' strokeWidth={1} '+(active?'shadow="0 1 2 #14232B40, 0 4 12 #14232B26"':''));
 const states=['crumb-rest','crumb-arrow','search-active','path-active','rename-active','rename-blur-menu','rename-error','filename-tooltip','dark-input'];
 return states.map((state,i)=>{
  let jsx=(state==='crumb-arrow'?navigationScenes(api)[2].jsx:searchScenes(api)[state==='search-active'?1:0].jsx);
  jsx=jsx.replace('w={320} h={32} px={8} flex="row" items="center" rounded={8}', 'w={336} h={36} px={4} py={4} flex="row" items="center" rounded={10} bg='+C('surface/canvas'))
   .replaceAll('h={32} px={4} rounded={8}','h={28} px={4} rounded={8}')
   .replace('stroke='+C('focus/ring')+' strokeWidth={2}','stroke='+C('border/subtle')+' strokeWidth={1} shadow="0 1 2 #14232B40, 0 4 12 #14232B26"');
  let overlay='';
  if(state==='path-active')overlay=F('Path editing / elevated focus',field('Path input','C:\\Users\\Simo\\Documents'),'position="absolute" x={80} y={6}');
  if(state==='rename-active'||state==='rename-blur-menu'||state==='rename-error'){
   const body=tile(state==='rename-blur-menu'?'研究资料':'研究记录',0,{selected:true,editing:state!=='rename-blur-menu'})+tile('项目文件',1,{selected:state==='rename-blur-menu'});
   overlay=F('Rename interaction / click outside commits',body,'position="absolute" x={184} y={44} w={400} h={300} bg='+C('surface/canvas'));
   if(state==='rename-blur-menu')overlay+=menu('file',{x:350,y:200});
   if(state==='rename-error')overlay+=F('Rename validation / retained value',T('名称不能包含 /',{role:'status/negative',size:12}),'position="absolute" x={196} y={204} w="hug" px={12} py={16} rounded={12} bg='+C('surface/floating')+' shadow="0 4 12 #14232B1F"');
  }
  if(state==='filename-tooltip')overlay=F('Tooltip / only filename hit region',T('项目研究记录_最终核对稿.pdf',{size:12}),'position="absolute" x={268} y={192} px={12} py={16} rounded={12} bg='+C('surface/floating')+' stroke='+C('border/subtle')+' strokeWidth={1} shadow="0 4 12 #14232B1F"');
  if(state==='dark-input')overlay=F('Input family / layered active and inactive',field('Inactive input','文件名',false)+spacer(24)+field('Active input','研究记录')+spacer(24)+field('Validation error','研究/记录',true,true),'position="absolute" x={520} y={160} px={12} py={16} flex="col" rounded={12} bg='+C('surface/floating'));
  if(overlay)jsx=jsx.slice(0,-8)+overlay+'</Frame>';
  jsx=jsx.replaceAll('stroke='+C('focus/ring')+' strokeWidth={2}','stroke='+C('border/subtle')+' strokeWidth={1} shadow="0 1 2 #14232B40, 0 4 12 #14232B26"');
  const key=String(93+i)+'-'+state,name=String(93+i)+' · v13 '+state;
  return{key,name,mode:state==='dark-input'?'Dark':'Light',jsx};
 });
}
