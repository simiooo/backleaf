"""Record the exact native inputs after successful checks/build, before packaging."""
from pathlib import Path
import hashlib,json,subprocess,datetime

root=Path(__file__).resolve().parents[2]
def git(*args):
    return subprocess.check_output(['git',*args],cwd=root,text=True,encoding='utf-8').strip()
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
if git('status','--porcelain'):
    raise SystemExit('Commit the source and design before recording a build')
paths=git('ls-files','src','Cargo.toml','Cargo.lock','rust-toolchain.toml',
          'design/color-system.json','design/layout-system.json').splitlines()
version=next(line.split('"')[1] for line in (root/'Cargo.toml').read_text('utf-8').splitlines() if line.startswith('version = '))
binary=root/'target/release/backleaf.exe'
manifest={
    'version':version,'source_commit':git('rev-parse','HEAD'),
    'gpui_revision':git('-C','vendor/zed','rev-parse','HEAD'),
    'recorded_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'compiler':subprocess.check_output(['rustc','--version'],cwd=root,text=True).strip(),
    'profile':'release; --locked','exe_sha256':sha(binary),'exe_bytes':binary.stat().st_size,
    'inputs':[{'path':p,'sha256':sha(root/p)} for p in paths],
    'check_logs':[{'path':f'{name}.log','sha256':sha(root/f'.local/v9-native/{name}.log')} for name in ['fmt','clippy','tests','build']],
    'check_scope':'Source checks and CPU model tests only; visible native/staging acceptance is recorded separately.'
}
output=root/'.local/native-build-manifest.json'
output.write_text(json.dumps(manifest,indent=2),encoding='utf-8')
print(json.dumps({'output':str(output),'version':version,'source_commit':manifest['source_commit'],'exe_sha256':manifest['exe_sha256']}))
