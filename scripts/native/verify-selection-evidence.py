"""Verify the archived SIM-953 evidence without opening a window."""
from pathlib import Path
import argparse,hashlib,json,subprocess

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--git-revision',help='also verify exact committed bytes, e.g. HEAD')
args=parser.parse_args()

folder=Path(__file__).resolve().parents[2]/'docs/evidence/native-selection'
def read(name):return json.loads((folder/name).read_text('utf-8-sig'))
manifest=read('manifest.json')
for item in manifest['files']:
    path=(folder/item['path']).resolve()
    assert path.is_relative_to(folder.resolve()),item['path']
    assert hashlib.sha256(path.read_bytes()).hexdigest()==item['sha256'],item['path']
    assert path.stat().st_size==item['bytes'],item['path']
    if args.git_revision:
        blob=subprocess.run(['git','show',f"{args.git_revision}:docs/evidence/native-selection/{item['path']}"],capture_output=True,check=True,cwd=folder.parents[2]).stdout
        assert hashlib.sha256(blob).hexdigest()==item['sha256'],f"Git blob: {item['path']}"
        assert len(blob)==item['bytes'],f"Git size: {item['path']}"
build,stage=read('build.json'),read('staging.json')
assert manifest['source_commit']==build['source_commit']==stage['source_commit']
assert manifest['exe_sha256']==build['exe_sha256']==stage['exe_sha256']
assert manifest['artifact_sha256']==stage['artifact_sha256']
assert stage['installed_hash_matches'] and stage['cli_smoke_passed'] and stage['interactive_smoke_passed']
reports=list((folder/'reports').glob('*.json'))
assert len(reports)==13
for path in reports:
    data=json.loads(path.read_text('utf-8'))
    assert data['session_initially_locked'] is False and data['session_locked_at_end'] is False,path.name
    assert data['max_pending_frame_callbacks']<=1 and data['hidden_animation_requests']==0,path.name
    if data['scripted_scroll']:
        assert data['scene_cpu']['p95_ms']<=(16.67 if data['logical_viewport'][0]>1120 else 8.33),path.name
        assert data['frame_callback_interval']['p95_ms']<=20,path.name
assert all(item['passed'] for item in read('ui-smoke.json')['cases'])
published=read('published.json')
assert published['target_commit']==manifest['source_commit']
assert published['tag_commit']==manifest['source_commit']
assert published['is_prerelease'] and not published['is_draft']
assert published['uploaded_asset_digests_match']
assert published['downloaded_bytes_verified'] is False
print(json.dumps({'source_commit':manifest['source_commit'],'evidence_files':len(manifest['files']),'reports':len(reports),'git_revision':args.git_revision,'result':'passed'}))
