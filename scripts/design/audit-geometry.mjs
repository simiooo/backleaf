import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
const version=process.argv[2]??'v8',project=resolve(import.meta.dir,'../..');
const io=new IORegistry(BUILTIN_IO_FORMATS),file=resolve(project,`design/Backleaf-${version}.fig`);
const {graph}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});populateAllFigPages(graph);
const absolute=n=>{let x=n.x,y=n.y,p=graph.getNode(n.parentId);while(p&&p.type!=='PAGE'&&p.type!=='DOCUMENT'){x+=p.x;y+=p.y;p=graph.getNode(p.parentId);}return{x,y,width:n.width,height:n.height};};
const records=[];
for(const page of graph.getPages().slice(44)){
 const walk=node=>{
  if(/File state \/|File hit area|File visual container|Primary action|Hover action|Input focus|Floating menu|Menu row|Task panel \/|Task cancel \/|Task file row/.test(node.name)){
   const icon=graph.getChildren(node.id).find(c=>c.name.startsWith('Icon slot')||c.name.startsWith('Icon identity'));
   let art=icon?.name.startsWith('Icon slot')?graph.getChildren(icon.id)[0]:icon;
   const visual=graph.getChildren(node.id).find(c=>c.name.startsWith('File visual container'));
   const children=graph.getChildren(node.id);
   const firstText=n=>n.type==='TEXT'?n:graph.getChildren(n.id).map(firstText).find(Boolean);
   const axis=node.name.startsWith('Task panel')?{
    title:absolute(firstText(children[0])).x-absolute(node).x,
    progress:absolute(children[2]).x-absolute(node).x,
    counts:absolute(firstText(children[4])).x-absolute(node).x,
    filename:absolute(firstText(children[6])).x-absolute(node).x,
   }:null;
   records.push({page:page.name,name:node.name,role:node.name.split(' /')[0],width:node.width,height:node.height,
    padding:{top:node.paddingTop,right:node.paddingRight,bottom:node.paddingBottom,left:node.paddingLeft},gap:node.itemSpacing,radius:node.cornerRadius,
    ...(art?{iconInset:{top:absolute(art).y-absolute(node).y,left:absolute(art).x-absolute(node).x}}:{}),
    ...(visual?{visual:{width:visual.width,height:visual.height,top:visual.y,left:visual.x}}:{}),...(axis?{axis}:{})});
  }
  for(const child of graph.getChildren(node.id))walk(child);
 };walk(page);
}
const checks=[];
if(version==='v9'){
 const l=JSON.parse(await readFile(resolve(project,'design/layout-system.json'),'utf8'));
 const check=(name,pass)=>{checks.push({name,pass});if(!pass)throw Error('Geometry: '+name);};
 const byRole=role=>records.filter(r=>r.role===role);
 check('Every file keeps a 112 px hit area',byRole('File hit area').every(r=>r.height===l.grid.cellHeight&&r.radius===0));
 check('File surfaces repeat 8/8/12/8 padding, 8 gap and 10 radius',byRole('File visual container').every(r=>r.padding.top===l.file.visualPaddingTop&&r.padding.bottom===l.file.visualPaddingBottom&&r.padding.left===l.file.visualPaddingX&&r.padding.right===l.file.visualPaddingX&&r.gap===l.file.iconNameGap&&r.radius===l.file.radius));
 check('Content surfaces hug one or two lines',byRole('File visual container').every(r=>r.height===92||r.height===108));
 check('Icon starts 8 px inside surface',byRole('File visual container').every(r=>r.iconInset.top===l.file.visualPaddingTop));
 check('Floating container uses 12 radius',byRole('Floating menu').every(r=>r.radius===l.floating.radius&&r.padding.top===l.floating.paddingY&&r.padding.left===l.floating.paddingX));
 check('Menu rows repeat 32 height, 8 radius and 8 horizontal inset',byRole('Menu row').every(r=>r.height===l.floating.rowHeight&&r.radius===l.floating.rowRadius&&r.padding.left===l.floating.rowPaddingX));
 check('Menu text inherits content inset 12/16',l.floating.paddingX+l.floating.rowPaddingX===l.content.paddingX&&l.floating.paddingY+(l.floating.rowHeight-16)/2===l.content.paddingY);
 check('Controls repeat 32 height and 8 radius',records.filter(r=>['Primary action','Hover action','Input focus'].includes(r.role)).every(r=>r.height===l.control.hitSize&&r.radius===l.control.radius));
 check('Task panels repeat 12/16 padding and 12 radius',byRole('Task panel').every(r=>r.radius===l.panel.radius&&r.padding.left===l.panel.paddingX&&r.padding.top===l.panel.paddingY));
 check('Task title, progress, counts and filenames share the same actual left axis',byRole('Task panel').every(r=>Object.values(r.axis).every(x=>Math.abs(x-l.panel.paddingX)<.01)));
 check('Task cancel repeats shared control geometry',byRole('Task cancel').every(r=>r.height===l.control.hitSize&&r.radius===l.control.radius));
}
await writeFile(resolve(project,`design/${version}/geometry-audit.json`),JSON.stringify({version,method:'actual reopened OpenPencil node geometry, not JSX intentions',checks,records},null,2));
const unique=[...new Map(records.map(r=>{const v={...r};delete v.page;delete v.name;return[JSON.stringify(v),v]})).values()];
console.log(JSON.stringify({version,records:records.length,checks,...(process.argv.includes('--compact')?{}:{representative:unique.slice(0,12)})},null,2));
