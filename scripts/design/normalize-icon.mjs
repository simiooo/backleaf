import {initCanvasKit,createCanvasKitRasterCodec} from '@open-pencil/core/io/formats/raster';

// Optical sizing is a presentation transform: keep original source art, trim
// transparent padding, and fit the visible alpha bounds into a 40px/44px slot.
// Do not synthesize, recolor, trace, or paint over the generated artwork.
function smoothMatte(decoded){
 const {width:w,height:h,data}=decoded,n=w*h,labels=new Int32Array(n),queue=new Int32Array(n);let label=0,best=0,bestCount=0;
 // Select the main opaque object; image-generation cutouts can contain isolated
 // white/cyan fragments far outside the intended icon.
 for(let start=0;start<n;start++)if(!labels[start]&&data[start*4+3]>=128){
  label++;let begin=0,end=1;queue[0]=start;labels[start]=label;
  while(begin<end){const i=queue[begin++],x=i%w,y=(i/w)|0;for(const j of [x?i-1:-1,x+1<w?i+1:-1,y?i-w:-1,y+1<h?i+w:-1])if(j>=0&&!labels[j]&&data[j*4+3]>=128){labels[j]=label;queue[end++]=j;}}
  if(end>bestCount){bestCount=end;best=label;}
 }
 if(!best)throw Error('Empty icon matte');
 let mask=new Uint8Array(n);for(let i=0;i<n;i++)mask[i]=+(labels[i]===best);
 const radius=Math.max(1,Math.round(Math.min(w,h)*.003));
 function filter(source,dilate){
  const stride=w+1,integral=new Int32Array((w+1)*(h+1));
  for(let y=0;y<h;y++){let sum=0;for(let x=0;x<w;x++){sum+=source[y*w+x];integral[(y+1)*stride+x+1]=integral[y*stride+x+1]+sum;}}
  const result=new Uint8Array(n),full=(radius*2+1)**2;
  for(let y=0;y<h;y++)for(let x=0;x<w;x++){const l=Math.max(0,x-radius),t=Math.max(0,y-radius),r=Math.min(w,x+radius+1),b=Math.min(h,y+radius+1),sum=integral[b*stride+r]-integral[t*stride+r]-integral[b*stride+l]+integral[t*stride+l];result[y*w+x]=+(dilate?sum>0:sum===full);}
  return result;
 }
 // Opening removes thin matte fringes; closing fills tiny edge chips. The radius
 // is below 0.2 UI pixels after sizing, not a redesign of the source silhouette.
 mask=filter(filter(mask,false),true);mask=filter(filter(mask,true),false);
 const clean=data.slice();
 for(let i=0;i<n;i++){
  clean[i*4+3]=mask[i]?255:0;
  if(mask[i]&&data[i*4+3]<128){const x=i%w,y=(i/w)|0;let found=-1;for(let d=1;d<=radius*3&&found<0;d++)for(let dy=-d;dy<=d&&found<0;dy++)for(let dx=-d;dx<=d;dx++){const xx=x+dx,yy=y+dy,j=yy*w+xx;if(xx>=0&&xx<w&&yy>=0&&yy<h&&data[j*4+3]>=128){found=j;break;}}if(found>=0)for(let c=0;c<3;c++)clean[i*4+c]=data[found*4+c];}
 }
 return{width:w,height:h,data:clean,matte:{components:label,mainPixels:bestCount,radius}};
}
export async function normalizeIcon(bytes,size=192,{cleanMatte=false}={}){
 const ck=await initCanvasKit(),codec=createCanvasKitRasterCodec(ck),decoded=codec.decode(bytes);
 if(!decoded)throw Error('Cannot decode icon');
 const prepared=cleanMatte?smoothMatte(decoded):decoded;
 let left=decoded.width,top=decoded.height,right=0,bottom=0;
 for(let y=0;y<decoded.height;y++)for(let x=0;x<decoded.width;x++)if(prepared.data[(y*decoded.width+x)*4+3]>=128){left=Math.min(left,x);top=Math.min(top,y);right=Math.max(right,x+1);bottom=Math.max(bottom,y+1);}
 if(right<=left||bottom<=top)throw Error('Empty icon');
 // Include an antialias edge around the significant alpha boundary.
 left=Math.max(0,left-2);top=Math.max(0,top-2);right=Math.min(decoded.width,right+2);bottom=Math.min(decoded.height,bottom+2);
 const limit=size*40/44,scale=limit/Math.max(right-left,bottom-top),w=(right-left)*scale,h=(bottom-top)*scale;
 const cleanBytes=cleanMatte?codec.encodePNG(prepared):bytes;
 const surface=ck.MakeSurface(size,size),image=ck.MakeImageFromEncoded(cleanBytes);if(!surface||!image)throw Error('Icon surface unavailable');
 try{
  const canvas=surface.getCanvas();canvas.clear(ck.TRANSPARENT);
  canvas.drawImageRectOptions(image,ck.LTRBRect(left,top,right,bottom),ck.LTRBRect((size-w)/2,(size-h)/2,(size+w)/2,(size+h)/2),ck.FilterMode.Linear,ck.MipmapMode.None,null);surface.flush();
  const snapshot=surface.makeImageSnapshot();try{const encoded=snapshot.encodeToBytes(ck.ImageFormat.PNG,100);if(!encoded)throw Error('Cannot encode normalized icon');return{bytes:new Uint8Array(encoded),sourceBounds:[left,top,right,bottom],displayInkBox:[w*44/size,h*44/size],slot:44,inkLimit:40,matte:prepared.matte};}finally{snapshot.delete();}
 }finally{image.delete();surface.delete();}
}
