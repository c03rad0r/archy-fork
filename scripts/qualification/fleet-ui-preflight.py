#!/usr/bin/env python3
"""Read-only Fleet artifact/live-state preflight; never deploys or restarts."""
import argparse,hashlib,json,os,pathlib,subprocess,tarfile,urllib.request,datetime
P=pathlib.Path
ap=argparse.ArgumentParser();ap.add_argument('--host',required=True,choices=['archi-dev-box','yaya-server']);ap.add_argument('--artifact',required=True,type=P);ap.add_argument('--out',required=True,type=P);ap.add_argument('--previous',type=P);a=ap.parse_args()
assert subprocess.check_output(['hostname'],text=True).strip()==a.host
assert not a.out.exists(),'Refuse to overwrite earlier preflight evidence'
def digest(p):
 with open(p,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def property(name):return subprocess.check_output(['systemctl','show','archipelago','--value','-p',name],text=True).strip()
q=json.loads((a.artifact/'qualification-final.json').read_text());manifest=json.loads((a.artifact/'dist-inputs.json').read_text());archive=a.artifact/'qualified-ui.tar.gz'
assert q['production_build_passed'] and q['source_inputs_unchanged']
assert q['baseline_tests_passed']==1459 and q['baseline_test_files_passed']==177
assert q['delta_focused_tests_passed']==31 and q['delta_app_typecheck_passed']
assert digest(archive)==q['archive_sha256']=='063752912fdcae2d1abf2abaccd53bfad39acce23c9a5d896ce52ba1e1996080'
assert digest(a.artifact/'dist-inputs.json')==q['dist_manifest_sha256']
assert digest(a.artifact/'dist/index.html')==q['index_sha256']
seen=set()
with tarfile.open(archive) as tar:
 for member in tar:
  path=P(member.name)
  assert not path.is_absolute() and '..' not in path.parts and path.parts[0]=='dist'
  assert member.isdir() or member.isfile(),'Non-regular archive member'
  if member.isfile():
   rel=str(path.relative_to('dist'));assert rel in manifest and rel not in seen
   with tar.extractfile(member) as f:assert hashlib.file_digest(f,'sha256').hexdigest()==manifest[rel]
   seen.add(rel)
assert seen==set(manifest)
web=P('/opt/archipelago/web-ui');data=P('/var/lib/archipelago')
files={data/x for x in ['remember_secret','user-stopped.json','user-uninstalled.json','installed-apps.json','app-catalog.json','node-app-catalog.json']}
for root in (data,web,data/'catalogs',data/'content',data/'qualification'):
 if root.is_dir():files.update(p for p in root.glob('*catalog*') if p.is_file())
for root in (web/'packages',web/'aiui',web/'catalogs',data/'identity'):
 if root.is_dir():files.update(p for p in root.rglob('*') if p.is_file())
preserved={str(p):digest(p) if p.is_file() else None for p in sorted(files)}
ids=subprocess.check_output(['podman','ps','-aq'],text=True).split()
rows=json.loads(subprocess.check_output(['podman','inspect',*ids])) if ids else []
containers={c['Name']:{'id':c['Id'],'started_at':c['State']['StartedAt'],'status':c['State']['Status'],'running':c['State']['Running']} for c in rows}
pid=property('MainPID');assert pid.isdigit() and int(pid)>0
exe=P('/proc')/pid/'exe'
try:running=digest(exe)
except PermissionError:running=subprocess.check_output(['sudo','-n','sha256sum',str(exe)],text=True).split()[0]
with urllib.request.urlopen('http://127.0.0.1/',timeout=20) as r:served=hashlib.file_digest(r,'sha256').hexdigest()
with urllib.request.urlopen('http://127.0.0.1:5678/health',timeout=20) as r:health=r.status
record={'captured_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'host':a.host,'read_only':True,'index_sha256':digest(web/'index.html'),'served_index_sha256':served,'backend_sha256':digest('/usr/local/bin/archipelago'),'running_backend_sha256':running,'manager':{'pid':pid,'active_state':property('ActiveState'),'sub_state':property('SubState'),'started_at':property('ExecMainStartTimestamp')},'health_http':health,'preserved':preserved,'containers':containers,'candidate_archive_sha256':q['archive_sha256'],'candidate_index_sha256':q['index_sha256'],'artifact_members_verified':len(seen),'backend_qualified':False,'deploy_ready':False}
assert served==record['index_sha256'],'Served UI differs from disk'
assert running==record['backend_sha256'],'Running backend differs from installed binary'
assert health==200
fd=os.open(a.out,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'w') as f:json.dump(record,f,indent=2);f.write('\n')
summary={k:record[k] for k in ['captured_at','host','read_only','index_sha256','backend_sha256','running_backend_sha256','health_http','artifact_members_verified','backend_qualified','deploy_ready']};summary.update(preservation_paths=len(preserved),containers=len(containers))
if a.previous:
 old=json.loads(a.previous.read_text());summary['drift_from_previous']={
 'ui_changed':old.get('index_sha256')!=record['index_sha256'],
 'backend_changed':old.get('backend_sha256')!=record['backend_sha256'],
 'preservation_paths_changed':len([p for p in set(old.get('preserved',{}))|set(preserved) if old.get('preserved',{}).get(p)!=preserved.get(p)]),
 'container_id_or_start_changes':len([n for n in set(old.get('containers',{}))|set(containers) if (old.get('containers',{}).get(n) if isinstance(old.get('containers',{}).get(n),list) else [old.get('containers',{}).get(n,{}).get('id'),old.get('containers',{}).get(n,{}).get('started_at')])!=([containers[n]['id'],containers[n]['started_at']] if n in containers else None)])}
print(json.dumps(summary))
