import {readFile,writeFile} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {BUILTIN_IO_FORMATS,IORegistry} from '@open-pencil/core/io';
import {populateAllFigPages} from '@open-pencil/core/io/formats/fig';
import {headlessRenderNodes} from '@open-pencil/core/io/formats/raster';

const project=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const file=join(project,'design/Backleaf.fig');
const io=new IORegistry(BUILTIN_IO_FORMATS);
const {graph}=await io.readDocument({name:file,data:new Uint8Array(await readFile(file))});
populateAllFigPages(graph);
const page=graph.getPages().find(page=>page.name.startsWith('33 ·'));
if(!page)throw new Error('Task detail page missing');
function find(node){
 if(node.name==='Task panel / unified content axis')return node;
 for(const child of graph.getChildren(node.id)){const result=find(child);if(result)return result;}
}
const panel=find(page);
if(!panel)throw new Error('Task panel missing');
const png=await headlessRenderNodes(graph,page.id,[panel.id],{format:'PNG',scale:1});
if(!png)throw new Error('Panel rendering failed');
const manifest=JSON.parse(await readFile(join(project,'design/recovery-verification.json'),'utf8'));
const output=join(project,'design',manifest.previews,'task-panel-detail.png');
await writeFile(output,png);
console.log(JSON.stringify({output,width:panel.width,height:panel.height}));
process.exit(0);
