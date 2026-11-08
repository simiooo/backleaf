// Mechanical export only: preserve the generated RGBA, resize and package.
import {readFile, writeFile, mkdir, copyFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {initCanvasKit, createCanvasKitRasterCodec} from '@open-pencil/core/io/formats/raster';

const root = resolve(import.meta.dir, '../..');
const out = resolve(root, 'assets/brand/icon-v3');
await mkdir(out, {recursive: true});
const source = new Uint8Array(await readFile(resolve(root, 'assets/brand/backleaf-logo-v3.png')));
const ck = await initCanvasKit(), codec = createCanvasKitRasterCodec(ck);
const decoded = codec.decode(source);
if (!decoded || decoded.width !== decoded.height) throw Error('Expected square generated PNG');
let transparent = 0, opaque = 0;
for (let i = 3; i < decoded.data.length; i += 4) {
  if (decoded.data[i] === 0) transparent++;
  if (decoded.data[i] === 255) opaque++;
}
if (!transparent || !opaque) throw Error('Expected transparent padding and opaque artwork');
const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256], frames = [];
const image = ck.MakeImageFromEncoded(source);
if (!image) throw Error('Cannot load generated artwork');
for (const size of sizes) {
  const surface = ck.MakeSurface(size, size);
  if (!surface) throw Error('Cannot create icon surface');
  const canvas = surface.getCanvas();
  canvas.clear(ck.TRANSPARENT);
  canvas.drawImageRectOptions(image, ck.LTRBRect(0, 0, decoded.width, decoded.height), ck.LTRBRect(0, 0, size, size), ck.FilterMode.Linear, ck.MipmapMode.None, null);
  surface.flush();
  const snapshot = surface.makeImageSnapshot(), png = snapshot.encodeToBytes(ck.ImageFormat.PNG, 100);
  if (!png) throw Error('Cannot encode icon PNG');
  frames.push({size, bytes: new Uint8Array(png)});
  await writeFile(resolve(out, `${size}.png`), png);
  snapshot.delete();
  surface.delete();
}
image.delete();
let offset = 6 + 16 * frames.length;
const ico = new Uint8Array(offset + frames.reduce((n, f) => n + f.bytes.length, 0));
const view = new DataView(ico.buffer);
view.setUint16(2, 1, true);
view.setUint16(4, frames.length, true);
for (const [i, frame] of frames.entries()) {
  const at = 6 + i * 16;
  ico[at] = ico[at + 1] = frame.size === 256 ? 0 : frame.size;
  view.setUint16(at + 4, 1, true);
  view.setUint16(at + 6, 32, true);
  view.setUint32(at + 8, frame.bytes.length, true);
  view.setUint32(at + 12, offset, true);
  ico.set(frame.bytes, offset);
  offset += frame.bytes.length;
}
// Read back every ICO entry, including its payload offset and transparent corners.
for (let i = 0; i < frames.length; i++) {
  const at = 6 + i * 16, size = ico[at] || 256;
  const start = view.getUint32(at + 12, true), length = view.getUint32(at + 8, true);
  const frame = codec.decode(ico.slice(start, start + length));
  if (!frame || frame.width !== size || frame.height !== size) throw Error(`Invalid ICO entry ${size}`);
  if ([3, size * 4 - 1, (size * (size - 1)) * 4 + 3, size * size * 4 - 1].some(n => frame.data[n] !== 0)) throw Error(`Opaque corner at ${size}`);
}
await writeFile(resolve(root, 'assets/windows/backleaf-v3.ico'), ico);
await writeFile(resolve(root, 'assets/windows/backleaf.ico'), ico);
await copyFile(resolve(root, 'assets/brand/backleaf-logo-v3.png'), resolve(root, 'assets/brand/backleaf-logo.png'));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
await writeFile(resolve(out, 'manifest.json'), JSON.stringify({
  issue: 'SIM-1030', status: 'user_adopted', source: '../backleaf-logo-v3.png',
  sourceSha256: sha(source), sourceSize: [decoded.width, decoded.height],
  transparentPixels: transparent, opaquePixels: opaque,
  ico: '../../windows/backleaf-v3.ico', icoSha256: sha(ico),
  frames: frames.map(f => ({size: f.size, sha256: sha(f.bytes)})),
  validation: 'All nine ICO entry offsets, PNG dimensions and transparent corners passed.',
  scope: 'Full-canvas mechanical resize and PNG-compressed ICO; generated alpha preserved. User-adopted v3 copied to runtime PNG and ICO.'
}, null, 2));
console.log(JSON.stringify({sizes, sourceSize: [decoded.width, decoded.height], transparent, opaque, result: 'passed'}));
process.exit(0);
