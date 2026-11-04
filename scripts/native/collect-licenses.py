"""Bundle license texts for the actually resolved Windows dependency graph."""
import argparse,json,pathlib,re,shutil,subprocess
p=argparse.ArgumentParser();p.add_argument('output',type=pathlib.Path);a=p.parse_args()
out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
def cargo(*args):return subprocess.run(['cargo',*args,'--locked'],check=True,capture_output=True,encoding='utf-8').stdout
tree=cargo('tree','--target','x86_64-pc-windows-msvc','--edges','normal,build','--prefix','none','--format','{p}')
used=set()
for line in tree.splitlines():
    m=re.match(r'([^ ]+) v([^ ]+)',line)
    if m:used.add(m.groups())
metadata=json.loads(cargo('metadata','--format-version','1'))
packages=[];missing=[]
for pkg in sorted(metadata['packages'],key=lambda x:(x['name'],x['version'])):
    if (pkg['name'],pkg['version']) not in used or pkg['name']=='backleaf':continue
    root=pathlib.Path(pkg['manifest_path']).parent;dest=out/f"{pkg['name']}-{pkg['version']}";dest.mkdir(exist_ok=True)
    files=[f for f in root.iterdir() if f.is_file() and re.match(r'^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|UNLICENSE)',f.name,re.I)]
    if pkg.get('license_file'):
        lf=root/pkg['license_file']
        if lf.exists() and lf not in files:files.append(lf)
    for file in files:shutil.copyfile(file,dest/file.name)
    selected=None;source=None
    if not files:
        override=pathlib.Path('licenses/overrides')/dest.name
        if override.is_dir():
            files=list(override.iterdir())
            for file in files:shutil.copyfile(file,dest/file.name)
            source=json.loads(pathlib.Path('licenses/overrides/sources.json').read_text(encoding='utf-8'))[dest.name]
        elif 'Apache-2.0' in (pkg.get('license') or ''):
            selected='Apache-2.0';source='https://www.apache.org/licenses/LICENSE-2.0'
            license_path=pathlib.Path('vendor/zed/crates/gpui/LICENSE-APACHE')
            shutil.copyfile(license_path,dest/'LICENSE-APACHE');files=[license_path]
            (dest/'ATTRIBUTION.txt').write_text(pkg['name']+' '+pkg['version']+'\nDeclared license: '+pkg['license']+'\nSelected license: Apache-2.0\nAuthors: '+', '.join(pkg.get('authors',[]))+'\nRepository: '+str(pkg.get('repository'))+'\n',encoding='utf-8')
        else:missing.append(f"{pkg['name']} {pkg['version']}")
    packages.append({'name':pkg['name'],'version':pkg['version'],'license':pkg.get('license'),'selected_license':selected,'notice_source':source,'repository':pkg.get('repository'),'authors':pkg.get('authors',[]),'license_files':[f.name for f in files]})
engine=pathlib.Path('assets/archive-engine')
engine_out=out/'7-Zip-26.05';engine_out.mkdir(exist_ok=True)
for name in ['LICENSE-7zip.txt','7z2605-src.tar.xz','provenance.json']:
    shutil.copyfile(engine/name,engine_out/name)
(engine_out/'RELINKING.txt').write_text('The unmodified 7-Zip dynamic library is distributed at lib/7z.dll. You may replace this x64 library with an ABI-compatible modified build without rebuilding Backleaf. Backleaf loads this explicit adjacent path and holds a read handle while the archive helper runs. Complete upstream source and license texts accompany this notice. Backleaf source and build instructions: https://github.com/simiooo/backleaf .\n',encoding='utf-8')
packages.append({'name':'7-Zip','version':'26.05','license':'LGPL-2.1-or-later; BSD-3-Clause; unRAR restriction','license_files':['LICENSE-7zip.txt'],'corresponding_source':'7z2605-src.tar.xz','provenance':'provenance.json','repository':'https://github.com/ip7z/7zip'})
(out/'manifest.json').write_text(json.dumps(packages,indent=2,ensure_ascii=False),encoding='utf-8')
(out/'README.txt').write_text('Third-party dependencies of the Windows Backleaf preview.\nGPUI packages are Apache-2.0. Zed application crates are not linked.\nSee each package folder and manifest.json for notices and license choices.\n',encoding='utf-8')
print(json.dumps({'packages':len(packages),'missing_license_files':missing}))
if missing:raise SystemExit('Missing license texts')
