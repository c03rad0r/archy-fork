#!/usr/bin/env python3
"""Prepared Fleet UI-only guard. Do not invoke deploy until backend qualification and live acceptance are complete."""
import hashlib,json,os,pathlib,shutil,shlex,subprocess,sys,tarfile,tempfile,time,urllib.request
P=pathlib.Path
mode, expected_host, receipt_path = sys.argv[1:4]
assert expected_host in ('archi-dev-box','yaya-server')
assert subprocess.check_output(['hostname'],text=True).strip()==expected_host
web=P('/opt/archipelago/web-ui');data=P('/var/lib/archipelago')
def digest(path):
    with open(path,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def run(*args):subprocess.run(args,check=True,stdout=subprocess.DEVNULL)
def inventory():
    ids=subprocess.check_output(['podman','ps','-aq'],text=True).split()
    rows=json.loads(subprocess.check_output(['podman','inspect',*ids])) if ids else []
    return {c['Name']:{'id':c['Id'],'started_at':c['State']['StartedAt'],'status':c['State']['Status'],'running':c['State']['Running']} for c in rows}
def preservation():
    names=[data/'remember_secret',data/'user-stopped.json',data/'user-uninstalled.json',data/'app-catalog.json',data/'node-app-catalog.json',data/'installed-apps.json']
    for root in (data,web,data/'catalogs',data/'content',data/'qualification'):
        if root.is_dir():names.extend(p for p in root.glob('*catalog*') if p.is_file())
    for root in (web/'packages',web/'aiui',web/'catalogs',data/'identity'):
        if root.is_dir():names.extend(p for p in root.rglob('*') if p.is_file())
    return {str(p):digest(p) if p.is_file() else None for p in set(names)}
def manager():
    def prop(name):return subprocess.check_output(['systemctl','show','archipelago','--value','-p',name],text=True).strip()
    return {'pid':prop('MainPID'),'active_state':prop('ActiveState'),'sub_state':prop('SubState'),'started_at':prop('ExecMainStartTimestamp')}
def running_digest():
    path='/proc/'+manager()['pid']+'/exe'
    try:return digest(path)
    except PermissionError:return subprocess.check_output(['sudo','-n','sha256sum',path],text=True).split()[0]
assert mode=='deploy' and len(sys.argv)==7, 'Use separate read-only preflight helper; deployment requires backend gate receipt'
before=json.loads(P(receipt_path).read_text());qualification=json.loads(P(sys.argv[4]).read_text());archive=P(sys.argv[5]);backend=json.loads(P(sys.argv[6]).read_text())
assert before['host']==expected_host
assert qualification['baseline_tests_passed']==1459 and qualification['baseline_test_files_passed']==177
assert qualification['delta_focused_tests_passed']==31 and qualification['delta_app_typecheck_passed']
assert qualification['production_build_passed'] and qualification['source_inputs_unchanged']
assert qualification['archive_sha256']=='063752912fdcae2d1abf2abaccd53bfad39acce23c9a5d896ce52ba1e1996080'
assert backend['host']==expected_host and backend['isolated_failed']==0 and backend['isolated_passed']>2006
assert backend['production_build_passed'] and backend['source_inputs_unchanged']
assert backend['test_source_manifest_sha256']==backend['build_source_manifest_sha256']
assert len(backend['test_source_manifest_sha256'])==64
required_tests={'unsigned_and_legacy_reports_cannot_assign_their_own_trust','collector_cannot_claim_another_record_identity_or_malformed_id','collector_history_suffix_cannot_overwrite_another_nodes_history','fleet_reads_do_not_promote_old_collector_spoofs_or_history'}
assert required_tests.issubset(set(backend['fleet_provenance_tests_passed']))
assert backend['contains_collector_provenance'] and backend['fleet_contract_readonly_checks_passed']
assert backend['deployment_preservation_passed'] and backend['embedded_helpers_match']
assert backend['running_sha256']==backend['binary_sha256']==before['backend_sha256']==before['running_backend_sha256']==running_digest()
assert backend['binary_sha256']!='7b85bb0135743200620963ae836867175283d57b6a8534fee2a19b72c3ce77c3', 'Old live backend is not qualified for Fleet labels'
assert manager()==before['manager'], 'Manager changed after fresh post-backend preflight'
assert digest(archive)==qualification['archive_sha256']
assert digest(web/'index.html')==before['index_sha256'],'Dashboard changed after preflight'
assert digest('/usr/local/bin/archipelago')==before['backend_sha256'],'Backend changed after preflight'
assert preservation()==before['preserved'],'Catalog or stopped/uninstalled intent changed after preflight'
assert inventory()==before['containers'],'Container changed after preflight'
backup=data/'support'/('fleet-ui-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime())+'-'+str(os.getpid()))
run('sudo','-n','install','-d','-m','700',str(backup))
run('sudo','-n','tar','--exclude=./aiui','--exclude=./packages','-czf',str(backup/'web-ui.tar.gz'),'-C',str(web),'.')
rollback_args=['tar','--exclude=./packages','--exclude=./aiui','--exclude=./catalogs']
rollback_args.extend('--exclude=./'+p.name for p in web.glob('*catalog*'))
rollback_args.extend(['-xzf',str(backup/'web-ui.tar.gz'),'-C',str(web)])
rollback='#!/bin/sh\nset -eu\n'+shlex.join(rollback_args)+'\n'
subprocess.run(['sudo','-n','tee',str(backup/'rollback.sh')],input=rollback.encode(),check=True,stdout=subprocess.DEVNULL)
run('sudo','-n','chmod','700',str(backup/'rollback.sh'))
staging=P(tempfile.mkdtemp(prefix='archy-fleet-ui-',dir='/var/tmp'));source=staging/'dist'
try:
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            path=P(member.name)
            assert not path.is_absolute() and '..' not in path.parts and path.parts[0]=='dist'
            assert member.isfile() or member.isdir()
        tar.extractall(staging,filter='data')
    assert digest(source/'index.html')==qualification['index_sha256']
    # Keep node-specific catalogs and existing standard/versioned package downloads.
    copy_code="""import pathlib,shutil,sys
s=pathlib.Path(sys.argv[1]);d=pathlib.Path('/opt/archipelago/web-ui')
for p in s.rglob('*'):
 r=p.relative_to(s)
 if r.parts[0] in ('aiui','packages','catalogs') or (len(r.parts)==1 and 'catalog' in p.name):continue
 if p.is_dir():(d/r).mkdir(parents=True,exist_ok=True)
 elif not (p.parent==s and p.name in ('index.html','sw.js','registerSW.js')):shutil.copy2(p,d/r)
"""
    run('sudo','-n','python3','-c',copy_code,str(source))
    for name in ('registerSW.js','sw.js','index.html'):
        if (source/name).exists():
            target=str(web/name)
            run('sudo','-n','install','-m','644',str(source/name),target+'.next')
            run('sudo','-n','mv',target+'.next',target)
    assert digest('/usr/local/bin/archipelago')==before['backend_sha256']
    assert preservation()==before['preserved'],'Catalog/session/intent preservation check failed'
    assert inventory()==before['containers'],'Container preservation check failed'
    assert manager()==before['manager'] and running_digest()==backend['binary_sha256']
    with urllib.request.urlopen('http://127.0.0.1/',timeout=30) as response:served=hashlib.file_digest(response,'sha256').hexdigest()
    assert served==qualification['index_sha256']
    with urllib.request.urlopen('http://127.0.0.1:5678/health',timeout=15) as response:assert response.status==200
    time.sleep(15)
    assert digest('/usr/local/bin/archipelago')==before['backend_sha256']
    assert preservation()==before['preserved'],'Delayed catalog/session/intent/package/aiui check failed'
    assert inventory()==before['containers'],'Delayed container preservation check failed'
    assert manager()==before['manager'] and running_digest()==backend['binary_sha256']
    print(json.dumps({'host':expected_host,'deployed':True,'index_sha256':served,'backend_unchanged':True,
        'catalog_session_intents_packages_aiui_preserved':True,'delayed_stability_seconds':15,'containers_unchanged':True,'rollback':str(backup/'rollback.sh')}))
except BaseException:
    run('sudo','-n',str(backup/'rollback.sh'))
    assert digest(web/'index.html')==before['index_sha256'],'Rollback index mismatch'
    print(json.dumps({'host':expected_host,'deployed':False,'rolled_back':True,'restored_index_sha256':digest(web/'index.html')}))
    raise
finally:shutil.rmtree(staging)
