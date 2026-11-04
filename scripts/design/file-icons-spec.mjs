// Original Backleaf vector artwork. Geometry is shared by editable OpenPencil
// masters and the embedded native assets; no fonts or external icon service.
export const accents={
 blue:['#376DA4','#A1C6EB'],green:['#37795B','#9ACDB3'],orange:['#B36B3F','#E5B28F'],
 red:['#B44949','#EAB1AE'],purple:['#80669E','#C6B7E3'],teal:['#367E83','#9CCDD0'],
 gold:['#99753C','#DAC396'],rose:['#A36386','#DAB2CC'],slate:['#64768C','#B5C5D9'],
};
const item=(key,label,sample,ext,color,glyph)=>({key,label,sample,extensions:ext.split(' ').filter(Boolean),color,glyph});
export const icons=[
 item('folder','文件夹','资料.png','','slate','folder'),
 item('generic','未知文件','未识别.custom','','slate','generic'),
 item('word','Word','报告.docx','doc docx docm dot dotx dotm odt rtf','blue','W'),
 item('spreadsheet','Excel','预算.xlsx','xls xlsx xlsm xlsb xlt xltx xltm xlam xla ods csv tsv','green','X'),
 item('presentation','PowerPoint','提案.pptx','ppt pptx pptm pps ppsx ppsm pot potx potm odp','orange','P'),
 item('pdf','PDF','合同.pdf','pdf','red','PDF'),
 item('application','可执行应用','Backleaf.exe','exe com scr appimage','blue','application'),
 item('installer','安装包','应用.msix','msi msp msix msixbundle appx appxbundle appinstaller msixupload appxupload cab dmg pkg deb rpm apk aab ipa','gold','installer'),
 item('image','图片','山川.heic','jpg jpeg jpe jfif png apng gif webp bmp dib tif tiff svg svgz ico icns heic heif avif jxl raw dng cr2 cr3 nef arw psd ai eps','green','image'),
 item('video','视频','旅程.mp4','mp4 m4v mkv mov avi wmv webm mpg mpeg mpe m2v m2ts mts ts vob flv ogv 3gp 3g2','purple','video'),
 item('audio','音乐','夜曲.flac','mp3 wav wave flac m4a aac ogg oga opus wma aiff aif alac ape mid midi amr','rose','audio'),
 item('archive','压缩包','资料.tar.gz','zip zipx 7z rar tar gz gzip bz bz2 bzip2 xz lz lzma zst zstd tgz tbz tbz2 txz tzst iso img','gold','archive'),
 item('book','电子书','山海经.epub','epub kepub mobi azw azw3 azw4 kfx fb2 fbz djvu djv cbz cbr cb7 cbt cba chm lit lrf prc','slate','book'),
 item('font','字体','思源宋体.otf','ttf ttc otf otc woff woff2 fnt fon pfb pfm','slate','font'),
 item('text','文本','笔记.txt','txt log nfo text','slate','generic'),
 item('code','源码','示例.src','src inc asm s r pl pm ex exs erl hrl clj cljs cljc hs fs fsx fsi vb vbs m mm matlab pas rkt jl nim zig sol sc scala','slate','code'),
 item('javascript','JavaScript','index.js','js mjs cjs','gold','JS'),
 item('typescript','TypeScript','types.ts','ts mts cts','blue','TS'),
 item('react','React','App.tsx','jsx tsx','teal','react'),
 item('python','Python','main.py','py pyi pyw ipynb','blue','PY'),
 item('rust','Rust','main.rs','rs','orange','RS'),
 item('go','Go','main.go','go','teal','GO'),
 item('c','C','main.c','c h','blue','C'),
 item('cpp','C++','main.cpp','cpp cxx cc c++ hpp hxx hh h++','blue','C++'),
 item('csharp','C#','Program.cs','cs csx','purple','C#'),
 item('java','Java','Main.java','java class jar','orange','JAVA'),
 item('kotlin','Kotlin','Main.kt','kt kts','purple','KT'),
 item('swift','Swift','Main.swift','swift','orange','SW'),
 item('dart','Dart','main.dart','dart','teal','DART'),
 item('php','PHP','index.php','php php3 php4 php5 php7 php8 phtml','purple','PHP'),
 item('ruby','Ruby','main.rb','rb erb rake gemspec','red','RB'),
 item('lua','Lua','main.lua','lua','blue','LUA'),
 item('html','HTML','index.html','html htm xhtml','orange','HTML'),
 item('css','CSS','style.css','css scss sass less','purple','CSS'),
 item('vue','Vue','App.vue','vue','green','VUE'),
 item('svelte','Svelte','App.svelte','svelte','orange','SV'),
 item('shell','Shell','build.sh','sh bash zsh fish bat cmd','slate','shell'),
 item('powershell','PowerShell','build.ps1','ps1 psm1 psd1','blue','PS'),
 item('sql','SQL','schema.sql','sql sqlite sqlite3 db','teal','SQL'),
 item('json','JSON','package.json','json jsonc json5 jsonl ndjson','gold','json'),
 item('yaml','YAML','config.yaml','yaml yml','rose','YML'),
 item('toml','TOML','Cargo.toml','toml','orange','TOML'),
 item('xml','XML','app.xml','xml xsl xslt xsd xaml plist resx csproj vbproj fsproj svgproj','orange','XML'),
 item('markdown','Markdown','README.md','md markdown mdx mdown mkd','slate','MD'),
 item('config','配置','settings.ini','ini cfg conf config properties env editorconfig gitignore gitattributes','slate','config'),
 item('docker','Docker','Dockerfile','dockerfile containerfile','blue','docker'),
 item('makefile','构建脚本','Makefile','mk make cmake gradle','slate','MK'),
];
// TS is both a video transport stream and TypeScript. Default to source code
// without reading file contents. Directories always win, including *.app.
icons.find(i=>i.key==='video').extensions=icons.find(i=>i.key==='video').extensions.filter(e=>e!=='ts'&&e!=='mts');
export const filenames={Dockerfile:'docker',Containerfile:'docker',Makefile:'makefile',GNUmakefile:'makefile','CMakeLists.txt':'makefile',Justfile:'makefile',Rakefile:'ruby',Gemfile:'ruby','.gitignore':'config','.gitattributes':'config','.editorconfig':'config','.env':'config'};
const alphabet={
 A:[[[0,7],[2.5,0],[5,7]],[[1,4.5],[4,4.5]]],B:[[[0,7],[0,0],[3.5,0],[5,1.5],[3.5,3.5],[0,3.5]],[[3.5,3.5],[5,5],[5,5.5],[3.5,7],[0,7]]],
 C:[[[5,1],[4,0],[1,0],[0,1],[0,6],[1,7],[4,7],[5,6]]],D:[[[0,7],[0,0],[3,0],[5,2],[5,5],[3,7],[0,7]]],
 E:[[[5,0],[0,0],[0,7],[5,7]],[[0,3.5],[4,3.5]]],F:[[[0,7],[0,0],[5,0]],[[0,3.5],[4,3.5]]],
 G:[[[5,1],[4,0],[1,0],[0,1],[0,6],[1,7],[5,7],[5,4],[3,4]]],H:[[[0,0],[0,7]],[[5,0],[5,7]],[[0,3.5],[5,3.5]]],
 I:[[[0,0],[5,0]],[[2.5,0],[2.5,7]],[[0,7],[5,7]]],J:[[[0,0],[5,0],[5,5.5],[3.5,7],[1.5,7],[0,5.5]]],
 K:[[[0,0],[0,7]],[[5,0],[0,4],[5,7]]],L:[[[0,0],[0,7],[5,7]]],M:[[[0,7],[0,0],[2.5,3.5],[5,0],[5,7]]],
 N:[[[0,7],[0,0],[5,7],[5,0]]],O:[[[1,0],[4,0],[5,1],[5,6],[4,7],[1,7],[0,6],[0,1],[1,0]]],
 P:[[[0,7],[0,0],[4,0],[5,1],[5,2.5],[4,3.5],[0,3.5]]],Q:[[[1,0],[4,0],[5,1],[5,6],[4,7],[1,7],[0,6],[0,1],[1,0]],[[3,5],[6,8]]],
 R:[[[0,7],[0,0],[4,0],[5,1],[5,2.5],[4,3.5],[0,3.5]],[[2.5,3.5],[5,7]]],S:[[[5,1],[4,0],[1,0],[0,1],[0,2.5],[1,3.5],[4,3.5],[5,4.5],[5,6],[4,7],[1,7],[0,6]]],
 T:[[[0,0],[5,0]],[[2.5,0],[2.5,7]]],U:[[[0,0],[0,6],[1,7],[4,7],[5,6],[5,0]]],V:[[[0,0],[2.5,7],[5,0]]],
 W:[[[0,0],[1,7],[2.5,3.5],[4,7],[5,0]]],X:[[[0,0],[5,7]],[[5,0],[0,7]]],Y:[[[0,0],[2.5,3.5],[5,0]],[[2.5,3.5],[2.5,7]]],
 Z:[[[0,0],[5,0],[0,7],[5,7]]],'+':[[[0,3.5],[5,3.5]],[[2.5,1],[2.5,6]]],'#':[[[1.5,0],[.5,7]],[[4.5,0],[3.5,7]],[[0,2],[5,2]],[[0,5],[5,5]]],
};
function letters(label){
 const scale=Math.min(2,22/(label.length*6-1)),y=27-scale*3.5,x=24-(label.length*6-1)*scale/2;
 return [...label].map((c,i)=>(alphabet[c]??alphabet.C).map(line=>line.map(([a,b],j)=>(j?'L':'M')+(x+(i*6+a)*scale).toFixed(2)+' '+(y+b*scale).toFixed(2)).join('')).join('')).join('');
}
const symbols={
 generic:'M17 23h14M17 28h14M17 33h9',code:'m20 22-5 5 5 5m8-10 5 5-5 5m-3-12-3 16',
 application:'M16 22h16v12H16ZM16 25h16M19 23.5h.1M22 23.5h.1m-2 5 3 2-3 2m6 0h3',
 installer:'m15 24 9-5 9 5v10l-9 5-9-5Zm0 0 9 5 9-5m-9 5v10m-4.5-17.5 9 5',
 image:'M15 21h18v14H15Zm0 11 5-6 5 5 3-3 5 7M29 24a1.4 1.4 0 1 0 0 .1Z',
 video:'M15 21h18v14H15ZM15 25h18M15 31h18m-13-10v14m8-14v14m-6-9 5 3-5 3Z',
 audio:'M22 33V21l10-2v12M22 24l10-2m-10 11a3.2 2.4 0 1 1-6.4 0 3.2 2.4 0 1 1 6.4 0m10-2a3.2 2.4 0 1 1-6.4 0 3.2 2.4 0 1 1 6.4 0',
 archive:'M23 18h3m-5 3h3m-1 3h3m-5 3h3m-1 3h3m-4 3h4v5h-4Z',
 book:'M15 20h15a3 3 0 0 1 3 3v14H18a3 3 0 0 1-3-3Zm3 0v17m0-4h15m-12-9h8m-8 4h6',
 font:'M15 23v-3h13v3m-6.5-3v16m-4 0h8m3-11v9a2 2 0 0 0 3 2m-5-9h7',
 react:'M34 27c0 2-4.5 3.6-10 3.6S14 29 14 27s4.5-3.6 10-3.6S34 25 34 27Zm-5-8.7c1.7 1-1 5.7-3.9 10.5s-6 7.8-7.7 6.9 0-5.7 2.9-10.5 7-7.8 8.7-6.9Zm-10 0c-1.7 1 1 5.7 3.9 10.5s6 7.8 7.7 6.9 0-5.7-2.9-10.5-7-7.8-8.7-6.9ZM24 26.8h.1',
 shell:'m16 22 6 5-6 5m8 1h8',json:'M20 20h-2v5l-2 2 2 2v5h2m8-14h2v5l2 2-2 2v5h-2m-4-10h.1m-.1 6h.1',
 config:'M16 22h16m-16 6h16m-16 6h16M21 20v4m7 2v4m-9 2v4',
 docker:'M16 25h4v4h-4Zm4 0h4v4h-4Zm4 0h4v4h-4Zm0-4h4v4h-4Zm4 4h4v4h-4ZM14 30h20c-1 5-5 7-10 7s-9-2-10-7Z',
};
export function artwork(entry,dark=false){
 const theme=dark?1:0,paper=dark?'#27323E':'#FAFCFE',stroke=dark?'#BAD0E3':'#58758D',accent=accents[entry.color][theme];
 if(entry.key==='folder')return[{d:'M5 13a3 3 0 0 1 3-3h10l4 5h18a3 3 0 0 1 3 3v19a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3Z',fill:dark?'#799BB8':'#BBD0E1',stroke,width:1.2},{d:'M5 19h38',fill:'none',stroke,width:1}];
 return [
  {d:'M12 5h16l10 10v26a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z',fill:paper,stroke,width:1.2},
  {d:'M28 5v10h10',fill:'none',stroke,width:1.2},
  {d:symbols[entry.glyph]??letters(entry.glyph),fill:'none',stroke:entry.key==='generic'||entry.key==='text'?stroke:accent,width:entry.glyph.length>3?1.35:1.6},
 ];
}
export function svg(entry,dark=false){return `<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 48 48">${artwork(entry,dark).map(p=>`<path d="${p.d}" fill="${p.fill}" stroke="${p.stroke}" stroke-width="${p.width}" stroke-linecap="round" stroke-linejoin="round"/>`).join('')}</svg>`;}
