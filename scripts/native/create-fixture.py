"""Create real, disposable NTFS directory entries for the native enumeration/render probe."""
import argparse,json,pathlib,time
p=argparse.ArgumentParser();p.add_argument('--count',type=int,default=100000);p.add_argument('--root',type=pathlib.Path,required=True);a=p.parse_args()
root=a.root.resolve();root.mkdir(parents=True,exist_ok=True)
marker=root/'.backleaf-render-fixture'
if any(root.iterdir()) and not marker.exists():raise SystemExit('Refusing to use a non-fixture directory')
marker.write_text('Disposable Backleaf native rendering fixture\n',encoding='utf-8')
started=time.perf_counter()
for i in range(a.count):
    name=f'文件_{i:08}_项目资料_👩🏽‍💻.txt' if i%101==0 else f'archive_{i:08}.txt'
    (root/name).touch(exist_ok=True)
print(json.dumps({'count':a.count,'directory_entries':a.count+1,'seconds':time.perf_counter()-started,'root':str(root)},ensure_ascii=True))
