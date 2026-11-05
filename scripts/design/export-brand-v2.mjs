// Mechanical PNG resizing and ICO packaging. Preserve the generated RGBA artwork;
// do not redraw, recolor, remove its matte or synthesize visual details.
import {readFile,writeFile,mkdir,copyFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {initCanvasKit,createCanvasKitRasterCodec} from '@open-pencil/core/io/formats/raster';
const root=resolve(import.meta.dir,'../..'),out=resolve(root,'assets/brand/icon-v2');await mkdir(out,{recursive:true});
const source=new Uint8Array(await readFile(resolve(root,'assets/brand/backleaf-logo-v2.png'))),ck=await initCanvasKit(),codec=createCanvasKitRasterCodec(ck);
const decoded=codec.decode(source);if(!decoded)throw Error('PNG decoding failed');
let transparent=0,opaque=0;
for(let i=3;i<decoded.data.length;i+=4){if(decoded.data[i]===0)transparent++;if(decoded.data[i]===255)opaque++;}
if(!transparent||!opaque)throw Error('Expected both transparent padding and opaque artwork');
const sizes=[16,20,24,32,40,48,64,128,256],frames=[],image=ck.MakeImageFromEncoded(source);
if(!image)throw Error('Cannot load generated artwork');
for(const size of sizes){
 const surface=ck.MakeSurface(size,size);if(!surface)throw Error('Cannot create icon surface');
 const canvas=surface.getCanvas();canvas.clear(ck.TRANSPARENT);
 canvas.drawImageRectOptions(image,ck.LTRBRect(0,0,decoded.width,decoded.height),ck.LTRBRect(0,0,size,size),ck.FilterMode.Linear,ck.MipmapMode.None,null);surface.flush();
 const snapshot=surface.makeImageSnapshot(),png=snapshot.encodeToBytes(ck.ImageFormat.PNG,100);if(!png)throw Error('Cannot encode icon PNG');
 frames.push({size,bytes:new Uint8Array(png)});await writeFile(resolve(out,`${size}.png`),png);snapshot.delete();surface.delete();
}
image.delete();
let offset=6+16*frames.length;const ico=new Uint8Array(offset+frames.reduce((n,f)=>n+f.bytes.length,0)),view=new DataView(ico.buffer);view.setUint16(2,1,true);view.setUint16(4,frames.length,true);
for(const [i,frame]of frames.entries()){
 const at=6+i*16;ico[at]=ico[at+1]=frame.size===256?0:frame.size;view.setUint16(at+4,1,true);view.setUint16(at+6,32,true);view.setUint32(at+8,frame.bytes.length,true);view.setUint32(at+12,offset,true);ico.set(frame.bytes,offset);offset+=frame.bytes.length;
 const decodedFrame=codec.decode(frame.bytes);if(decodedFrame.width!==frame.size||decodedFrame.height!==frame.size)throw Error('Frame dimensions changed');
}
await writeFile(resolve(root,'assets/windows/backleaf.ico'),ico);await copyFile(resolve(root,'assets/brand/backleaf-logo-v2.png'),resolve(root,'assets/brand/backleaf-logo.png'));
await writeFile(resolve(out,'manifest.json'),JSON.stringify({source:'../backleaf-logo-v2.png',sourceSha256:createHash('sha256').update(source).digest('hex'),sourceSize:[decoded.width,decoded.height],transparentPixels:transparent,opaquePixels:opaque,icoSha256:createHash('sha256').update(ico).digest('hex'),frames:frames.map(f=>({size:f.size,sha256:createHash('sha256').update(f.bytes).digest('hex')})),scope:'Generated RGBA preserved, full-canvas mechanical resize and PNG-compressed ICO only.'},null,2));
console.log(JSON.stringify({frames:sizes,transparentPixels:transparent,opaquePixels:opaque}));process.exit(0);
