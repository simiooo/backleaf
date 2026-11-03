import {createTaskPresentation,taskColors} from './task-panel.mjs';
const escape=value=>String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
export function createBulkScreens(ui){
 const {T,I,B,A,F,S,C,app,grid,status,tile}=ui;
 const view=createTaskPresentation(ui);
 const million=1_000_000;
 const longName='2026年跨区域项目合作资料与验收记录_第三轮修改后保留完整附件说明_最终确认版本_请在审阅前核对所有修改意见和文件来源_20261103.pdf';
 const fullPath='C:\\Users\\Simo\\Documents\\项目资料\\跨区域合作\\2026年度归档\\第三阶段\\'+longName;
 const files=Array.from({length:66},(_,i)=>({name:`归档${String(42101+i).padStart(6,'0')}.${['pdf','jpg','txt','docx'][i%4]}`,type:'file'}));
 const active={target:'文档',countText:'42,108 / 100,000 项',sizeText:'9.8 GB / 20.0 GB',progress:.49,state:'active'};
 const jobs=Array.from({length:24},(_,i)=>i===0?active:i===1?{target:'回收站',countText:'6,200 / 28,600 项',sizeText:'1.8 GB / 6.4 GB',progress:.28125,state:'active'}:{target:`备份 / 项目${i+1}`,countText:`0 / ${8200+i*20} 项`,sizeText:'0 B / 4.2 GB',progress:0,state:'waiting'});
 const longRows=[{name:longName,state:'active'},{name:'AnnualReport_'+('Q4_0123456789'.repeat(8))+'.xlsx',state:'waiting'},{name:'项目合同与附件汇总_第二轮审阅意见_全部合并版本_待签署副本.docx',state:'failed'},...view.samples.slice(3)];
 function directory({width=1120,selected=false,excludeOne=false,items=files}={}){
  const columns=width<700?5:11,shown=items.slice(0,width<700?30:66);
  return `<Frame name="Virtual directory viewport / ${million} total" w="fill" grow={1} flex="col" overflow="hidden"><Frame name="Scrollable directory clip / balanced vertical margins" position="absolute" x={12} y={16} w={${width-24}} h={616} overflow="hidden"><Frame name="Visible rows only / stable 116px pitch" position="absolute" x={0} y={0} w={${width-24}} grid columns="repeat(${columns}, 1fr)" columnGap={8} rowGap={16}>${shown.map((f,i)=>tile({...f,selected:selected&&(!excludeOne||i!==4),focus:selected&&i===0})).join('')}</Frame></Frame><Frame name="Directory scroll track" position="absolute" x={${width-8}} y={16} w={4} h={616} bg="#F0F2F4" rounded={2}><Rectangle name="Thumb / minimum visible size" x={0} y={86} w={4} h={24} bg="#BBC3CA" rounded={2}/></Frame></Frame>`;
 }
 function page(name,{width=1120,selected=false,excludeOne=false,items=files,overlay='',totalJobs=0,footer='1,000,000 项'}={}){
  return app(name,directory({width,selected,excludeOne,items}),{width,title:'Simo / 归档',jobs:totalJobs,footer:status(footer),overlay});
 }
 function tip(name,state,{x=592,y=178,width=360}={}){
  return `<Frame name="Filename tooltip / full value and state / stable item identity" position="absolute" x={${x}} y={${y}} w={${width}} maxH={224} flex="col" px={8} py={12} gap={6} bg="#30373D" rounded={4} shadow="0 2 8 #18222B24"><Text name="Full tooltip filename" font="Noto Sans SC" size={12} lineHeight={18} color="#FFFFFF" w="fill" textAutoResize="height">${escape(name)}</Text>${T(state,11,'#D3D9DE','w="fill" h={16}')}</Frame>`;
 }
 function fullDetails(){
  const field=(label,value)=>`<Frame name="Full details field / ${label}" w="fill" flex="col" gap={6}><Frame name="Field heading" w="fill" h={28} flex="row" items="center">${T(label,11,C.muted)}${F}${['文件名','位置'].includes(label)?B('copy','Copy full '+label,28):''}</Frame><Text name="Full detail value / no ellipsis" font="Noto Sans SC" size={12} lineHeight={18} color="${C.ink}" w="fill" textAutoResize="height">${escape(value)}</Text></Frame>`;
  return app('41 · 长文件名与路径 / 详情完整展示',`<Frame name="Directory and full details" w="fill" grow={1} flex="row"><Frame name="File area" grow={1} h="fill" flex="col">${grid([{name:longName,type:'file',selected:true},...files.slice(0,9)],{width:800})}</Frame><Frame name="Full details / selectable wrapped values" w={320} h="fill" flex="col" px={12} py={16} gap={16} bg="#FAFBFC"><Frame name="Details heading" w="fill" h={24} flex="row" items="center">${T('详情',13)}${F}${B('close','Close file details',24)}</Frame>${field('文件名',longName)}${field('位置',fullPath)}${field('状态','无权访问')}${field('大小','1.8 GB')}</Frame></Frame>`,{title:'Simo / 归档',footer:status('已选 1 项')});
 }
 const motionSheet=`<Frame name="42 · 文件名状态与流光 / 可编辑关键帧" w={1120} h={480} flex="col" px={12} py={16} gap={16} bg="#F4F6F7">${T('文件名状态与流光',16)}${[[.12,'0 ms'],[.5,'600 ms'],[.88,'1,200 ms']].map(([phase,label])=>`<Frame name="Flow phase / ${label}" w={600} h={28} flex="row" gap={12} items="center"><Frame name="Phase label" w={88} h={16}>${T(label,11,C.muted)}</Frame><Frame name="Editable gradient filename" grow={1} h={18} flex="row">${view.filename('归档_042109_跨区域合作项目验收记录.pdf',{state:'active',phase,layer:'Flow keyframe'})}</Frame></Frame>`).join('')}<Frame name="Reduced motion" w={600} h={28} flex="row" gap={12} items="center"><Frame name="Reduced motion label" w={88} h={16}>${T('减少动态效果',11,C.muted)}</Frame><Frame name="Static filename" grow={1} h={18} flex="row">${view.filename('归档_042109_跨区域合作项目验收记录.pdf',{state:'active',motion:false})}</Frame></Frame>${T('1,800 ms 循环；只对可见活动项绘制；共用时钟，离屏与隐藏时停止。',12,C.muted)}<Frame name="State variants / spec only" w={600} flex="col" gap={4}>${['done','waiting','failed'].map((state,i)=>`<Frame name="${state}" w="fill" h={24} flex="row"><Frame name="State label" w={88} h={16}>${T(['已完成','等待','失败'][i],11,C.muted)}</Frame><Frame name="Name" grow={1} h={18} flex="row">${view.filename('项目资料_最终签署版本.pdf',{state})}</Frame></Frame>`).join('')}</Frame></Frame>`;
 const scaleSheet=`<Frame name="38 · 大规模渲染约束" w={1120} h={500} flex="col" px={12} py={16} gap={16} bg="#F4F6F7">${T('可见渲染与交互预算',16)}${[['网格','只创建视口与前后2行；按行高计算范围，不能遍历百万文件。'],['文字','仅测量可见项；布局缓存有界；省略展示，tooltip与详情读取原值。'],['流光','单个时钟，只对可见活动项更新；隐藏/减少动态效果时停止。'],['复用','稳定文件身份与视图代次；滚动后旧tooltip和旧异步结果失效。'],['指标','真实GPUI帧时间、输入延迟和GPU占用仍需SIM-935实机验收。']].map(([label,value])=>`<Frame name="Rendering rule / ${label}" w="fill" flex="row" gap={12}><Frame name="Rule label" w={56} h={20}>${T(label,12)}</Frame><Frame name="Rule value" grow={1} h={20} flex="row">${T(value,12,C.muted,'w="fill"')}</Frame></Frame>`).join('')}</Frame>`;
 const failures=[{name:'归档_000097_合作协议扫描件.pdf',state:'failed'},{name:'归档_000194_合同附件最终版.jpg',state:'failed'},{name:'归档_000291_年度资料汇总说明.txt',state:'failed'}];
 return {
  '31-million-directory':page('31 · 百万目录 / 全选与排除',{selected:true,excludeOne:true,footer:'已选择 999,999 / 1,000,000 项'}),
  '32-large-queue':page('32 · 总览 / 仅目标进度个数与大小',{totalJobs:24,overlay:view.panel(jobs)}),
  '33-large-task-detail':page('33 · 文件名明细 / 统一左轴与颜色流光',{totalJobs:24,overlay:view.panel([active],{expanded:true})}),
  '34-large-failures':page('34 · 未完成文件 / 颜色与按需详情',{totalJobs:1,overlay:view.panel([{...active,countText:'90,000 / 100,000 项',sizeText:'18.0 GB / 20.0 GB',progress:.9,state:'error'}],{expanded:true,rows:failures})}),
  '35-large-cancelling':page('35 · 取消中 / 保留实际完成数据',{totalJobs:24,overlay:view.panel([{...active,state:'cancelling'}],{expanded:true,rows:view.samples.map(x=>({...x,state:x.state==='active'?'cancelled':x.state})),motion:false})}),
  '36-narrow-large-queue':page('36 · 窄窗总览 / 限高与省略',{width:560,totalJobs:24,overlay:view.panel(jobs,{width:560})}),
  '37-large-clipboard':page('37 · 剪贴板准备 / 仅真实数据',{selected:true,totalJobs:1,overlay:view.panel([{target:'系统剪贴板',countText:'184,320 / 1,000,000 项',sizeText:'2.8 GB / 20.0 GB',progress:.18432,state:'active'}]),footer:'已选择 1,000,000 项'}),
  '38-scale-contract':scaleSheet,
  '39-long-grid-tooltip':page('39 · 长文件名网格 / 省略与tooltip',{items:[{name:longName,type:'file'},{name:'AnnualReport_'+('0123456789'.repeat(12))+'.xlsx',type:'file'},...files.slice(0,31)],overlay:tip(longName,'PDF 文档',{x:16,y:166})}),
  '40-long-task-tooltip':page('40 · 长文件名任务列表 / 省略与tooltip',{totalJobs:24,overlay:view.panel([{...active,target:'文档 / 跨区域项目合作资料与完整验收文档归档目录'}],{expanded:true,rows:longRows})+tip(longName,'正在复制',{x:592,y:178})}),
  '41-full-details':fullDetails(),
  '42-text-motion':motionSheet,
  '43-unknown-size':page('43 · 总量未知 / 不伪造大小与百分比',{totalJobs:1,overlay:view.panel([{target:'文档',countText:'18,432 / — 项',sizeText:'9.8 GB / —',state:'preparing'}])}),
  '44-narrow-long-text':page('44 · 窄窗长文本 / 保持统一左轴',{width:560,totalJobs:24,items:[{name:longName,type:'file'},...files.slice(0,29)],overlay:view.panel([{...active,target:'文档 / 跨区域项目合作资料与完整验收文档归档目录'}],{width:560,expanded:true,rows:longRows})})
 };
}
