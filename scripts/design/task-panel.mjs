// One composition owns horizontal padding, vertical rhythm and every content axis.
export const taskColors={done:'#2E3439',waiting:'#70777E',active:'#31566F',sweep:'#4B6E86',failed:'#A62D29',cancelled:'#70777E'};
const escape=value=>String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');

export function createTaskPresentation(ui){
  const {T,I,B,A,F,C}=ui;
  function filename(name,{state='waiting',phase=.45,props='',layer='Task filename',motion=true}={}){
    const color=taskColors[state]||taskColors.done;
    const stops=[[taskColors.active,0],[taskColors.active,Math.max(0,phase-.18)],[taskColors.sweep,phase],[taskColors.active,Math.min(1,phase+.18)],[taskColors.active,1]];
    const fill=state==='active'&&motion?`fills={[linearGradient(${JSON.stringify(stops)})]}`:`color="${color}"`;
    return `<Text name="${escape(layer+' / '+state+' / '+name)}" font="Noto Sans SC" size={12} lineHeight={18} ${fill} w="fill" h={18} truncate={true} maxLines={1} textAutoResize="none" ${props}>${escape(name)}</Text>`;
  }
  function targetOf(job){
    if(job.target)return job.target;
    if(job.title?.includes('回收站'))return '回收站';
    return job.title?.split('→').at(-1)?.trim()||'文档';
  }
  function summary(job,{expanded=false,width=376}={}){
    const final=['done','cancelled','error'].includes(job.state);
    const unknown=job.state==='preparing'||job.progress===undefined;
    const progress=Math.max(0,Math.min(1,job.progress??0));
    const count=job.countText||job.count||'0 / — 项';
    const bytes=job.sizeText||'— / —';
    const action=job.state==='cancelling'?I('pending',C.muted,12):final?(job.state==='error'?`<Frame name="Retry incomplete items" h={30} px={6} flex="row" items="center">${T('重试',12)}</Frame>`:''):`<Frame name="Cancel batch / never dismiss panel" h={30} px={6} flex="row" items="center">${T('取消',12,C.muted)}</Frame>`;
    return `<Frame name="Task summary / target progress count size only" w="fill" flex="col" gap={6}>
      <Frame name="Task target row" w="fill" h={30} flex="row" items="center" gap={4}>
        <Frame name="Task target / disclosure and full target tooltip" grow={1} h={30} flex="row" items="center" gap={4}>
          <Text name="Task target text" font="Noto Sans SC" size={12} lineHeight={16} color="${C.ink}" w="fill" h={16} truncate={true} maxLines={1} textAutoResize="none">${escape(targetOf(job))}</Text>${I(expanded?'chevronUp':'down',C.muted,10)}
        </Frame>${action}
      </Frame>
      <Frame name="Task progress / common left edge" w="fill" h={2} bg="#E5E8EB"><Rectangle name="${unknown?'Unknown total activity segment':'Actual completed progress'}" w={${unknown?56:Math.round(width*progress)}} h={2} bg="${job.state==='error'?taskColors.failed:C.progress}"/></Frame>
      <Frame name="Task quantitative dimensions" w="fill" grid columns="repeat(2, 1fr)" columnGap={12}>
        <Text name="Task count / common left edge" font="Noto Sans SC" size={12} lineHeight={18} color="${C.muted}" w="fill" h={18} maxLines={1} truncate={true}>${escape(count)}</Text>
        <Text name="Task size / second aligned column" font="Noto Sans SC" size={12} lineHeight={18} color="${C.muted}" w="fill" h={18} maxLines={1} truncate={true}>${escape(bytes)}</Text>
      </Frame>
    </Frame>`;
  }
  const samples=[
    {name:'归档_042109.pdf',state:'active'},
    {name:'归档_042110.pdf',state:'waiting'},
    {name:'归档_042111.pdf',state:'waiting'},
    {name:'归档_042112.pdf',state:'failed'},
    {name:'归档_042113.pdf',state:'done'},
    {name:'归档_042114.pdf',state:'waiting'},
    {name:'归档_042115.pdf',state:'waiting'},
    {name:'归档_042116.pdf',state:'waiting'}
  ];
  function resultList(rows=samples,{motion=true}={}){
    return `<Frame name="Task files / visible filename-only rows" w="fill" flex="col" gap={0}>${rows.map(row=>`<Frame name="Task file row / ${row.state}" w="fill" h={28} flex="row" items="center">${filename(row.name,{state:row.state,motion})}</Frame>`).join('')}</Frame>`;
  }
  function panelContent(contents,{width=1120,name='Task panel / unified content axis',scroll=false}={}){
    return `<Frame name="${name}" position="absolute" x={${Math.max(8,width-544)}} y={52} w={400} maxH={420} flex="col" px={12} py={16} gap={12} bg="${C.bg}" stroke="${C.line}" strokeWidth={1} rounded={6} shadow="0 3 10 #18222B18">${contents}${scroll?'<Frame name="Task queue scrollbar" position="absolute" x={396} y={16} w={2} h={296} bg="#EDF0F2"><Rectangle name="Queue thumb" x={0} y={0} w={2} h={44} bg="#BBC3CA"/></Frame>':''}</Frame>`;
  }
  function panel(jobs,{width=1120,expanded=false,rows=samples,motion=true}={}){
    const visible=expanded?jobs.slice(0,1):jobs.slice(0,4);
    let contents=expanded?summary(visible[0],{expanded}):`<Frame name="Task summaries / one gap owner" w="fill" flex="col" gap={16}>${visible.map(job=>summary(job)).join('')}</Frame>`;
    if(expanded)contents+=resultList(rows,{motion})+`<Frame name="Task detail tools / no additional messages" w="fill" h={28} flex="row" justify="end" gap={4}>${B('search','Find task files',28)}${B('filter','Filter task files',28)}</Frame>`;
    return panelContent(contents,{width,scroll:!expanded&&jobs.length>4});
  }
  return {filename,summary,resultList,panelContent,panel,samples};
}
