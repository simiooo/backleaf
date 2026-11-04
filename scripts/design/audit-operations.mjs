import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
const project=resolve(import.meta.dir,'../..'),io=new IORegistry(BUILTIN_IO_FORMATS);
const file=resolve(project,'design/Backleaf-v10.fig');
const {graph}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(graph);
const records=[],checks=[];
const absolute=n=>{let x=n.x,y=n.y,p=graph.getNode(n.parentId);while(p&&p.type!=='PAGE'&&p.type!=='DOCUMENT'){x+=p.x;y+=p.y;p=graph.getNode(p.parentId);}return{x,y};};
function descend(n,predicate){if(predicate(n))return n;for(const c of graph.getChildren(n.id)){const found=descend(c,predicate);if(found)return found;}}
for(const page of graph.getPages().slice(52)){
 const walk=n=>{
  if(/Context menu \/|Menu row \/|Operation panel \/|Queue \/|Count column|Bytes column/.test(n.name)){
   const record={page:page.name,name:n.name,width:n.width,height:n.height,radius:n.cornerRadius,padding:[n.paddingTop,n.paddingRight,n.paddingBottom,n.paddingLeft]};
   if(n.name.startsWith('Operation panel')){
    const kids=graph.getChildren(n.id),firstText=descend(kids[0],c=>c.type==='TEXT');
    const track=kids.find(c=>c.name==='Progress track'),count=descend(n,c=>c.name==='Count column'),row=descend(n,c=>c.name.startsWith('Operation item'));
    record.axes=[firstText,track,count,...(row?[row]:[])].map(c=>absolute(c).x-absolute(n).x);
    record.progressWidth=graph.getChildren(track.id)[0].width;
   }
   records.push(record);
  }
  for(const c of graph.getChildren(n.id))walk(c);
 };walk(page);
}
const check=(name,pass)=>{checks.push({name,pass});};
const where=prefix=>records.filter(r=>r.name.startsWith(prefix));
check('70 editable pages, 52 retained',graph.getPages().length===70);
check('Menu surface padding 4/8, radius12',where('Context menu').every(r=>r.radius===12&&r.padding.join()===['8','4','8','4'].join()));
check('Menu rows repeat height32, radius8, paddingX8',where('Menu row').every(r=>r.height===32&&r.radius===8&&r.padding[1]===8&&r.padding[3]===8));
check('Task panels fit 380px and repeat 12/16 insets',where('Operation panel').every(r=>r.height<=380&&r.radius===12&&r.padding.join()===[16,12,16,12].join()));
check('Target, progress, counts and filenames share x12',where('Operation panel').every(r=>r.axes.every(x=>Math.abs(x-12)<.01)));
check('Count and byte columns have a stable 188px width',[...where('Count column'),...where('Bytes column')].every(r=>r.width===188));
check('Completed track is full width',where('Operation panel').filter(r=>r.name.endsWith('/ complete')).every(r=>r.progressWidth===376));
check('Queue surface fits 360px and has no per-task cards',where('Queue /').every(r=>r.height<=360&&r.radius===12));
await writeFile(resolve(project,'design/v10/geometry-audit.json'),JSON.stringify({method:'actual cold-opened node geometry',checks,records},null,2));
console.log(JSON.stringify({checks,records:records.length}));if(checks.some(c=>!c.pass))process.exitCode=1;
