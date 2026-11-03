import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {SceneGraph} from '@open-pencil/scene-graph';
import {FigmaAPI} from '@open-pencil/core/figma-api';
import {ALL_TOOLS} from '@open-pencil/core/tools';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {headlessRenderNodes} from '@open-pencil/core/io/formats/raster';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {measureDocumentText,editDocument} from './node_modules/@open-pencil/cli/src/headless.ts';
import {spacing,screens} from './layout.mjs';

const project=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const previewOnly=process.argv.includes('--preview-only');
const requested=process.env.BACKLEAF_DESIGN_PAGES?.split(',');
if(requested&&!previewOnly)throw new Error('Page filtering requires --preview-only; partial masters are forbidden');
const entries=Object.entries(screens).filter(([key])=>!requested||requested.includes(key));
if(!entries.length)throw new Error('No requested design pages');
const output=join(project,previewOnly?'.local/design-preview-v5':'design/v5');
await mkdir(output,{recursive:true});
const graph=new SceneGraph(),figma=new FigmaAPI(graph),io=new IORegistry(BUILTIN_IO_FORMATS);
const collection=figma.variables.createVariableCollection('Backleaf spacing');
for(const [name,value] of Object.entries(spacing)){
 const variable=figma.variables.createVariable(name,collection.id,'FLOAT');
 variable.setValueForMode(collection.defaultModeId,value);
}
await measureDocumentText(graph);
const renderer=ALL_TOOLS.find(t=>t.name==='render'),records=[];
for(const [index,[key,jsx]] of entries.entries()){
 const page=index===0?figma.currentPage:figma.createPage();
 figma.currentPage=page;
 await renderer.loadFonts(figma,{jsx});
 const result=await editDocument(graph,()=>renderer.execute(figma,{jsx}));
 if(result?.error)throw new Error(JSON.stringify(result));
 const roots=graph.getChildren(page.id);
 if(roots.length!==1)throw new Error('Unexpected screen count '+key);
 const root=roots[0];page.name=root.name;
 const image=await headlessRenderNodes(graph,page.id,[root.id],{format:'PNG',scale:1});
 if(!image)throw new Error('No image '+key);
 await writeFile(join(output,key+'.png'),image);
 const texts=[];const walk=n=>{if(n.type==='TEXT')texts.push(n.text);for(const c of graph.getChildren(n.id))walk(c)};walk(root);
 records.push({key,page:page.name,pageId:page.id,rootId:root.id,nodes:graph.countDescendants(root.id)+1,texts});
 console.log(JSON.stringify({stage:'rendered',key,nodes:records.at(-1).nodes}));
}
// Audit actual parent/child rectangles for every page.
const overflows=[],zeroText=[],fileMetrics=[],titlebars=[];
for(const node of graph.getAllNodes()){
 if(node.type==='TEXT'&&node.text&&node.height<1)zeroText.push({id:node.id,name:node.name});
 if(node.name==='Custom title bar / compact 40px'){
  const drag=graph.getChildren(node.id).find(c=>c.name.startsWith('Drag region'));
  titlebars.push({id:node.id,width:node.width,height:node.height,dragWidth:drag?.width});
 }
 if(node.name.startsWith('File identity /')){
  const kids=graph.getChildren(node.id),slot=kids.find(c=>c.name.startsWith('Icon slot')),label=kids.find(c=>c.name.startsWith('Filename /'));
  if(slot&&label)fileMetrics.push({id:node.id,name:node.name,width:node.width,height:node.height,labelAlignment:graph.getChildren(label.id)[0]?.textAlignHorizontal,iconHeight:slot.height,iconToLabel:label.y-slot.y-slot.height,leftAxisDelta:label.x-slot.x});
 }
 if(node.type!=='FRAME'||node.width<1||node.height<1)continue;
 for(const child of graph.getChildren(node.id)){
  if(child.layoutPositioning==='ABSOLUTE'||!['FRAME','TEXT'].includes(child.type)||child.visible===false)continue;
  const dx=Math.max(0,-child.x,child.x+child.width-node.width),dy=Math.max(0,-child.y,child.y+child.height-node.height);
  if(dx>0.75||dy>0.75)overflows.push({parent:node.name,child:child.name,dx,dy,parentSize:[node.width,node.height],childRect:[child.x,child.y,child.width,child.height]});
 }
}
const metricErrors=fileMetrics.filter(m=>Math.abs(m.iconToLabel-4)>0.75||Math.abs(m.leftAxisDelta)>0.75||m.iconHeight!==44||m.labelAlignment!=='CENTER');
const dragErrors=titlebars.filter(m=>m.dragWidth<31.5||m.height!==40);
const audit={verifiedAt:new Date().toISOString(),spacing,overflows,zeroText,fileMetrics,titlebars,metricErrors,dragErrors};
await writeFile(join(project,previewOnly?'.local/design-preview-audit.json':'design/spacing-audit.json'),JSON.stringify(audit,null,2));
if(overflows.length||zeroText.length||metricErrors.length||dragErrors.length){
 console.error(JSON.stringify({stage:'audit-failed',overflows,zeroText,metricErrors,dragErrors}));process.exit(2);
}
if(previewOnly){console.log(JSON.stringify({stage:'preview-checked',pages:records.length,overflows:0}));process.exit(0)}
await writeFile(join(project,'design/source/screens.json'),JSON.stringify(screens,null,2));
await writeFile(join(project,'design/source/spacing.json'),JSON.stringify(spacing,null,2));
figma.currentPage=figma.root.children[0];
const out=await io.writeDocument('fig',graph,{renderThumbnail:true});
const file=join(project,'design/Backleaf.fig');
await writeFile(file,out.data);
const {graph:reopened}=await io.readDocument({name:file,data:out.data});
populateAllFigPages(reopened);
const pages=reopened.getPages();
if(pages.length!==records.length)throw new Error('Page count changed after reopen');
for(let i=0;i<pages.length;i++){
 const roots=reopened.getChildren(pages[i].id);
 if(roots.length!==1||reopened.countDescendants(roots[0].id)+1!==records[i].nodes)throw new Error('Lost nodes '+pages[i].name);
}
await writeFile(join(project,'design/recovery-verification.json'),JSON.stringify({tool:'OpenPencil 0.17.0-beta.1',version:'design-v5-bulk-scale',previews:'v5',verifiedAt:new Date().toISOString(),master:'Backleaf.fig',spacingVariables:Object.keys(spacing).length,pages:records,roundtrip:'all pages and content nodes preserved'},null,2));
console.log(JSON.stringify({stage:'saved-and-reopened',file,pages:pages.length,nodes:records.reduce((a,r)=>a+r.nodes,0),fileMetrics:fileMetrics.length,titlebars:titlebars.length,overflows:0}));
process.exit(0);
