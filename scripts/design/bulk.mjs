// Scale states use a small, editable viewport sample, never a million scene nodes.
export function createBulkScreens(ui) {
  const {T,I,B,A,F,S,C,app,status,regular,tile}=ui;
  const count=1_000_000;
  const files=Array.from({length:66},(_,i)=>({
    name:`归档${String(42101+i).padStart(6,'0')}.${['pdf','jpg','txt','docx'][i%4]}`,
    type:'file'
  }));
  const jobs=Array.from({length:24},(_,i)=>({
    id:`bulk-${i}`,title:i===0?'复制 100,000 项 → 文档':i===1?'移至回收站 28,600 项':`移动 ${8200+i*20} 项 → 备份`,
    count:i===0?'42,108 项':i===1?'6,200 项':'',
    detail:i===0?'归档_042109.pdf · 正在复制':i===1?'正在处理：照片_006201.jpg':'等待目标设备空闲',
    progress:i===0?.42108:.21678,state:i<2?'active':'waiting'
  }));

  function directory({width=1120,selected=false,loading=false,excludeOne=false}={}) {
    const columns=width<700?5:11;
    const shown=files.slice(0,width<700?30:66);
    const gridWidth=width-24;
    return `<Frame name="Virtual directory viewport / ${count} total" w="fill" grow={1} flex="col" overflow="hidden">
      <Frame name="Visible grid rows only / 100px pitch" position="absolute" x={8} y={8} w={${gridWidth}} grid columns="repeat(${columns}, 1fr)" columnGap={6} rowGap={12}>${shown.map((f,i)=>tile({...f,selected:selected&&(!excludeOne||i!==4),focus:selected&&i===0})).join('')}</Frame>
      <Frame name="Directory scrollbar / independent of task list" position="absolute" x={${width-8}} y={8} w={4} h={630} bg="#F0F2F4" rounded={2}><Rectangle name="Proportional thumb / minimum visible size" x={0} y={${loading?0:86}} w={4} h={24} bg="#BBC3CA" rounded={2}/></Frame>
    </Frame>`;
  }

  function panel(contents,{width=1120,name='Bulk operation detail / bounded viewport'}={}) {
    return `<Frame name="${name}" position="absolute" x={${Math.max(8,width-544)}} y={44} w={400} maxH={380} flex="col" p={6} gap={4} bg="${C.bg}" stroke="${C.line}" strokeWidth={1} rounded={6} shadow="0 3 10 #18222B18">${contents}</Frame>`;
  }

  function heading(title,{cancel=false,back=true}={}) {
    return `<Frame name="Task detail heading / back to shared queue" w="fill" h={26} flex="row" gap={4} items="center">${back?B('back','Back to all operations',24):''}${T(title,12)}${F}${cancel?B('close','Cancel this batch',24):B('close','Close details / operations keep running',24)}</Frame>`;
  }

  function filter(label,active=false) {
    return `<Frame name="Status filter / ${label}" h={26} flex="col" px={4} justify="center">${T(label,11,active?C.ink:C.muted)}${active?'<Rectangle name="Active filter underline" w={24} h={1} bg="#697E8D"/>':''}</Frame>`;
  }

  function itemRow(name,state,{failed=false,current=false}={}) {
    return `<Frame name="Virtual result row / ${name}" w="fill" h={24} flex="row" gap={6} items="center">${I(failed?'warn':current?'pending':state==='等待'?'clock':'check',failed?C.error:C.muted,12)}
      <Frame name="Item identity / full path on focus or tooltip" grow={1} h={16} flex="row">${T(name,11,C.ink,'w="fill" h={16} maxLines={1}')}</Frame>
      <Frame name="Result status" w={72} h={16} flex="row" justify="end">${T(state,11,failed?C.error:C.muted)}</Frame>
    </Frame>`;
  }

  function detail({width=1120,cancelling=false}={}) {
    return panel(heading(cancelling?'正在取消复制…':'复制 100,000 项 → 文档',{cancel:!cancelling})+
      `<Frame name="Durable outcome summary" w="fill" h={18} flex="row" px={2} items="center">${T('42,108 项已完成 · 12 项失败',11)}${F}${cancelling?I('pending',C.muted,12):''}</Frame>
      <Frame name="Completed-item progress / known total" w="fill" h={2} bg="#E5E8EB"><Rectangle name="Completed items only" w={163} h={2} bg="${C.progress}"/></Frame>
      <Frame name="Status filters / one result collection" w="fill" flex="row" gap={4}>${filter('全部',true)}${filter('处理中')}${filter('失败 12')}${F}${B('search','Find a file in this operation',24)}</Frame>
      <Frame name="Virtual results viewport / 8 visible rows of 100000" w="fill" flex="col" gap={0}>${Array.from({length:8},(_,i)=>itemRow(`归档_${42109+i}.pdf`,i===0?(cancelling?'正在停止…':'正在复制'):i===3?'无权访问':'等待',{current:i===0,failed:i===3})).join('')}</Frame>
      <Frame name="Result cursor / stable while progress updates" w="fill" h={24} flex="row" px={2} items="center">${T('42,109–42,116 / 100,000',10,C.muted)}${F}${T(cancelling?'已完成的文件保留':'定位当前文件',11,C.muted)}</Frame>`,{width});
  }

  function failures(width=1120) {
    return panel(heading('复制结束 · 10,000 项未完成')+
      `<Frame name="Partial completion summary" w="fill" h={18} flex="row" px={2} items="center">${T('90,000 项已完成',11)}${F}${T('仅重试未完成项',11,C.muted)}</Frame>
      <Frame name="Failure reason filters / no per-file dialogs" w="fill" flex="col" gap={0}>
        ${[['无权访问','6,200'],['同名冲突','2,800'],['位置不可用','1,000']].map(([label,n])=>`<Frame name="Reason filter / ${label}" w="fill" h={22} flex="row" px={2} items="center">${T(label,11)}${F}${T(`${n} 项`,11,C.muted)}${I('down',C.muted,10)}</Frame>`).join('')}
      </Frame>
      <Frame name="Filter selection" w="fill" h={26} flex="row">${filter('全部未完成',true)}${F}${B('search','Find an incomplete file',24)}</Frame>
      <Frame name="Paged failures / only three visible of ten thousand" w="fill" flex="col">${itemRow('归档_000097.pdf','无权访问',{failed:true})}${itemRow('归档_000194.jpg','同名冲突',{failed:true})}${itemRow('归档_000291.txt','位置不可用',{failed:true})}</Frame>
      <Frame name="Failure actions" w="fill" h={28} flex="row" px={2} items="center" gap={6}>${T('1–3 / 10,000',10,C.muted)}${F}${A('重试未完成项',{strong:true})}</Frame>`,{width,name:'Large failure collection / bounded and filterable'});
  }

  function bulkPage(name,{width=1120,selected=false,loading=false,excludeOne=false,footer='1,000,000 项',overlay='',totalJobs=0}={}) {
    return app(name,directory({width,selected,loading,excludeOne}),{width,title:'Simo / 归档',jobs:totalJobs,footer:status(footer),overlay});
  }

  const prep=panel(heading('正在准备剪贴板…',{cancel:true,back:false})+
    `<Frame name="Clipboard preparation count" w="fill" h={22} flex="row" px={2} items="center">${T('184,320 / 1,000,000 项',12)}${F}${I('pending',C.muted,12)}</Frame>
     <Frame name="Preparation progress" w="fill" h={2} bg="#E5E8EB"><Rectangle name="Prepared selections" w={71} h={2} bg="${C.progress}"/></Frame>
     <Frame name="Clipboard ownership changes only after success" w="fill" h={18} flex="row" px={2}>${T('完成后可粘贴',11,C.muted)}</Frame>`);

  const scaleSheet=`<Frame name="38 · 大规模处理与性能验收" w={1120} h={500} flex="col" p={8} gap={10} bg="#F4F6F7">${T('大规模文件操作 · UI 与性能基线',16)}
    ${[
      ['规模','1万 / 10万 / 100万条目；24项操作；大量失败与长文件名。'],
      ['显示','固定行高虚拟网格；任务面板只渲染可见行；明细按页读取。'],
      ['操作','应用队列有界；按设备协调I/O；未知总量不显示伪百分比。'],
      ['反馈','进度合并到每秒10次；结果不丢；取消通过独立控制通道。'],
      ['验收','滚动帧时间、输入延迟、内存、队列深度、首屏、取消响应分别测量。'],
      ['边界','Shell进度点不等于字节；剪贴板完整路径数组另计内存。']
    ].map(([title,body])=>`<Frame name="Scale contract / ${title}" w="fill" flex="row" gap={8}><Frame name="Contract label" w={56} h={20}>${T(title,12)}</Frame><Frame name="Contract value" grow={1} h={20} flex="row">${T(body,12,C.muted,'w="fill"')}</Frame></Frame>`).join('')}
    ${T('数值为本轮工程初始目标；隔离模型验证不等于真实文件吞吐、GPUI帧率或系统验收。',12,C.muted)}
  </Frame>`;

  return {
    '31-million-directory':bulkPage('31 · 百万目录 / 全选与排除',{selected:true,excludeOne:true,footer:'已选择 999,999 / 1,000,000 项'}),
    '32-large-queue':regular({name:'32 · 24项任务 / 有界面板',title:'Simo / 归档',items:files.slice(0,33),jobs,footer:'2 项进行中 · 22 项等待'}),
    '33-large-task-detail':bulkPage('33 · 十万文件 / 按需展开结果',{totalJobs:24,overlay:detail(),footer:'2 项进行中 · 22 项等待'}),
    '34-large-failures':bulkPage('34 · 一万失败 / 筛选和重试',{totalJobs:1,overlay:failures(),footer:'90,000 项已完成 · 10,000 项未完成'}),
    '35-large-cancelling':bulkPage('35 · 大批次取消 / 等待后端停止',{totalJobs:24,overlay:detail({cancelling:true}),footer:'正在取消此任务 · 其他任务继续'}),
    '36-narrow-large-queue':regular({name:'36 · 窄窗 / 24项任务排队',title:'Simo / 归档',width:560,items:files.slice(0,30),jobs,footer:'2 项进行中 · 22 项等待'}),
    '37-large-clipboard':bulkPage('37 · 百万选择 / 剪贴板准备',{selected:true,overlay:prep,footer:'已选择 1,000,000 项 · 正在准备剪贴板'}),
    '38-scale-contract':scaleSheet
  };
}
