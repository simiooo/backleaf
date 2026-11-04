"""Verify the archived SIM-953 evidence without opening a window."""
from pathlib import Path
import hashlib,json

folder=Path(__file__).resolve().parents[2]/'docs/evidence/native-selection'
def read(name):return json.loads((folder/name).read_text('utf-8-sig'))
manifest=read('manifest.json')
for item in manifest['files']:
    path=(folder/item['path']).resolve()
    assert path.is_relative_to(folder.resolve()),item['path']
    assert hashlib.sha256(path.read_bytes()).hexdigest()==item['sha256'],item['path']
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
print(json.dumps({'source_commit':manifest['source_commit'],'evidence_files':len(manifest['files']),'reports':len(reports),'result':'passed'}))
