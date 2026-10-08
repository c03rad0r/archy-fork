"""Pure fixture coverage; imports the controller without invoking main/live tools."""
import importlib.util,json,pathlib,tempfile,unittest,uuid
MODULE=pathlib.Path(__file__).resolve().parents[2]/'scripts/indeehub-maintenance-controller.py'
spec=importlib.util.spec_from_file_location('maintenance_controller',MODULE);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
def members():
 return [{'name':name,'container_id':f'{n+1:064x}','image_id':'a'*64,'unit_sha256':'b'*64,'config_sha256':'c'*64,'running':True} for n,name in enumerate(module.NAMES)]
class MaintenanceTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=pathlib.Path(self.tmp.name);self.operation=str(uuid.uuid4());self.calls=[]
  holds=self.root/'update-transactions'/'holds';holds.mkdir(parents=True)
  for name in module.NAMES:(holds/name).write_text(self.operation)
  self.controller=module.Controller(self.root,self.operation,0,self.command_runner)
 def command_runner(self,argv,timeout,output):
  self.calls.append(argv)
  if argv[:2]==['podman','inspect']:
   member=next(m for m in members() if m['name']==argv[2]);return json.dumps([{'Id':member['container_id'],'Image':member['image_id'],'HostConfig':{'PortBindings':{}}}]).encode()
  raise AssertionError('Unexpected fixture command '+str(argv))
 def test_exact_seven_member_identity_is_required(self):
  self.assertEqual(len(module.validate_members(members())),7)
  for bad in [members()[:-1],members()+[members()[0]],[dict(members()[0],container_id='invalid'),*members()[1:]]]:
   with self.assertRaises(RuntimeError):module.validate_members(bad)
 def test_foreign_operation_cannot_release_or_overwrite_hold(self):
  path=self.root/'update-transactions'/'holds'/'indeedhub';path.write_text(str(uuid.uuid4()))
  with self.assertRaises(RuntimeError):self.controller.holds()
  self.assertEqual(self.calls,[])
 def test_failed_original_exposure_validation_is_not_skipped_on_retry(self):
  for _ in range(2):
   with self.assertRaisesRegex(RuntimeError,'frontend exposure'):self.controller.acquire(members())
   self.controller=module.Controller(self.root,self.operation,0,self.command_runner)
   self.assertFalse(self.controller.record.get('originals_validated',False))
  self.assertEqual(len(self.calls),2)
 def test_changed_original_terms_cannot_resume_saved_operation(self):
  self.controller.record={'operation_id':self.operation,'original_members':module.validate_members(members()),'phase':'Prepared'};self.controller.save()
  changed=members();changed[0]['config_sha256']='d'*64
  with self.assertRaisesRegex(RuntimeError,'terms changed'):self.controller.acquire(changed)
  self.assertEqual(self.calls,[])
 def test_release_lost_reply_is_idempotent_but_never_removes_foreign_fence(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Released','outcome':'committed'};c.save()
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  self.assertEqual(c.release('committed')['state'],'released');self.assertFalse(c.fence.exists())
  self.assertEqual(c.release('committed')['state'],'released')
  c.fence.write_text(str(uuid.uuid4()))
  with self.assertRaises(RuntimeError):c.release('committed')
  self.assertTrue(c.fence.exists())
 def test_backup_file_existence_never_substitutes_for_drain_evidence(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared','backup_complete':False};c.save()
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  (c.root/'backup').mkdir();(c.root/'backup'/'database.dump').write_bytes(b'not evidence')
  with self.assertRaisesRegex(RuntimeError,'Drain not complete'):c.verify()
 def completed_backup(self):
  c=self.controller
  c.record={'operation_id':self.operation,'phase':'Drained','backup_complete':True,
            'stopped':{name:{'confirmed':True} for name in module.NAMES},'artifacts':{}}
  c.save();c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  (c.root/'backup').mkdir()
  for name in ['database.dump',*(v+'.tar' for v in module.VOLUMES)]:
   path=c.root/'backup'/name;path.write_bytes(b'original')
   c.record['artifacts'][name]={'bytes':path.stat().st_size,'sha256':module.sha(path)}
  c.record['original_members']=members()
  c.record['database_before']={'operation_id':self.operation,'tables':{},'migrations':[]}
  c.record['backup_restore_verified']=c.backup_restore_terms()
  c.record['volume_restore_verified']=c.volume_restore_terms()
  c.save()
  return c
 def test_complete_backup_checksums_allow_verification(self):
  self.assertEqual(self.completed_backup().verify()['state'],'held')
 def test_restore_proof_must_match_fresh_dump_baseline_image_and_operation(self):
  c=self.completed_backup();proof=dict(c.record['backup_restore_verified'])
  for field in proof:
   c.record['backup_restore_verified']={**proof,field:'changed'}
   with self.assertRaisesRegex(RuntimeError,'restore is not verified'):c.verify()
   self.assertEqual(c.fence.read_text(),self.operation)
  c.record.pop('backup_restore_verified')
  with self.assertRaisesRegex(RuntimeError,'restore is not verified'):c.verify()
 def test_volume_restore_proof_must_match_all_archives_and_operation(self):
  import copy
  c=self.completed_backup();proof=copy.deepcopy(c.record['volume_restore_verified'])
  for field in ('operation_id',*module.VOLUMES):
   changed=copy.deepcopy(proof)
   if field=='operation_id':changed[field]='changed'
   else:changed['archives'][field+'.tar']='changed'
   c.record['volume_restore_verified']=changed
   with self.assertRaisesRegex(RuntimeError,'volume backup restore is not verified'):c.verify()
  c.record.pop('volume_restore_verified')
  with self.assertRaisesRegex(RuntimeError,'volume backup restore is not verified'):c.verify()
  self.assertEqual(c.fence.read_text(),self.operation)
 def test_foreign_volume_fixture_is_never_removed(self):
  c=self.completed_backup();name='volume-restore-'+'a'*32
  path=c.root/name;path.mkdir();(path/'owner').write_text(str(uuid.uuid4()))
  c.record['volume_restore_fixture']=name
  with self.assertRaisesRegex(RuntimeError,'ownership changed'):c.cleanup_volume_fixture()
  self.assertEqual(self.calls,[]);self.assertTrue(path.exists())
 def test_unsafe_archive_paths_and_links_are_rejected_before_extraction(self):
  import tarfile,io
  c=self.completed_backup();path=c.root/'unsafe.tar'
  cases=[('../escape',tarfile.REGTYPE,''),('/escape',tarfile.REGTYPE,''),
         ('link',tarfile.SYMTYPE,'../../escape'),('link',tarfile.LNKTYPE,'../escape'),
         ('device',tarfile.CHRTYPE,''),('hard',tarfile.LNKTYPE,'missing')]
  for name,kind,target in cases:
   with tarfile.open(path,'w') as archive:
    root=tarfile.TarInfo('.');root.type=tarfile.DIRTYPE;archive.addfile(root)
    entry=tarfile.TarInfo(name);entry.type=kind;entry.linkname=target;archive.addfile(entry)
   with self.assertRaises(RuntimeError):module.validate_volume_archive(path)
  with tarfile.open(path,'w') as archive:
   for name,kind,target in [('.',tarfile.DIRTYPE,''),('dir',tarfile.SYMTYPE,'safe'),('dir/file',tarfile.REGTYPE,'')]:
    entry=tarfile.TarInfo(name);entry.type=kind;entry.linkname=target;archive.addfile(entry)
  with self.assertRaisesRegex(RuntimeError,'writes through a link'):module.validate_volume_archive(path)
 def test_foreign_restore_fixture_is_never_removed(self):
  c=self.completed_backup();name='archy-backup-restore-'+'a'*32
  c.record['restore_fixture']={'name':name,'image_id':'a'*64}
  def command(argv,timeout,output):
   if argv[:2]==['podman','ps']:return b'container-id'
   if argv[:2]==['podman','inspect']:return json.dumps([{'Name':name,'Image':'a'*64,'Config':{'Labels':{'io.archipelago.backup.operation':str(uuid.uuid4())}}}]).encode()
   raise AssertionError('Unexpected mutation '+str(argv))
  c.runner=command
  with self.assertRaisesRegex(RuntimeError,'ownership changed'):c.cleanup_restore_fixture()
  self.assertIn('restore_fixture',c.record)
 def test_same_size_corruption_keeps_admission_closed(self):
  c=self.completed_backup();(c.root/'backup'/'database.dump').write_bytes(b'corrupt!')
  with self.assertRaisesRegex(RuntimeError,'checksum changed'):c.verify()
  self.assertEqual(c.fence.read_text(),self.operation);self.assertEqual(self.calls,[])
 def test_incomplete_or_unexpected_artifact_inventory_cannot_pass(self):
  c=self.completed_backup();original=dict(c.record['artifacts'])
  for artifacts in [{}, {k:v for k,v in original.items() if k!='database.dump'},
                    {**original,'../foreign':original['database.dump']}]:
   c.record['artifacts']=artifacts
   with self.assertRaisesRegex(RuntimeError,'inventory'):c.verify()
   self.assertEqual(c.fence.read_text(),self.operation)
 def test_forced_original_exit_never_marks_writer_completed(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared','original_members':module.validate_members(members())};c.save()
  def command(argv,timeout,output):
   if argv[:2]==['podman','inspect']:return self.command_runner(argv,timeout,output)
   if argv[:3]==['systemctl','--user','stop']:return b''
   if argv[:3]==['systemctl','--user','show']:return b'ActiveState=inactive\nResult=success\n'
   if argv[:2]==['podman','events']:return json.dumps({'ID':members()[0]['container_id'],'ContainerExitCode':137}).encode()
   raise AssertionError(argv)
  c.runner=command
  with self.assertRaisesRegex(RuntimeError,'not allowed for this writer'):c.graceful_stop('indeedhub')
  self.assertFalse(c.record['stopped']['indeedhub'].get('confirmed',False))
  self.assertTrue((self.root/'update-transactions'/'holds'/'indeedhub').exists())
 def test_failed_stop_can_restore_without_fabricating_completed_drain(self):
  c=self.controller;c.record={'operation_id':self.operation,'original_members':module.validate_members(members()),'phase':'Prepared','stopped':{'indeedhub':{'intent_at':1,'exit_code':137}}};c.save()
  runtime=c.data/'update-transactions'/'supervised'/(self.operation+'.json');module.atomic(runtime,{'phase':'Restoring','target_startup_began':False})
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  c.close_ingress=lambda: c.fence_matches()
  self.assertEqual(c.acquire(members(),recovery=True)['state'],'recovering')
  self.assertFalse(c.record.get('backup_complete',False))
  self.assertEqual(c.verify()['state'],'held')
  module.atomic(runtime,{'phase':'Restored','target_startup_began':False})
  self.assertEqual(c.release('aborted')['state'],'released')
  self.assertIn('No target startup',c.record['rollback_data_claim'])
 def test_target_started_rollback_requires_data_compatibility(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Recovering'};c.save()
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  module.atomic(c.data/'update-transactions'/'supervised'/(self.operation+'.json'),{'phase':'Restored','target_startup_began':True})
  with self.assertRaisesRegex(RuntimeError,'Data compatibility'):c.release('restored')
  self.assertTrue(c.fence.exists())
 def test_nginx_namespace_failure_cannot_create_admission_fence(self):
  def failed_dump(argv,timeout,output):
   self.assertEqual(argv,['sudo','-n','/usr/bin/systemd-run','--quiet','--wait',
                          '--pipe','--collect','--','/usr/sbin/nginx','-T'])
   raise module.subprocess.CalledProcessError(1,argv)
  self.controller.runner=failed_dump
  with self.assertRaises(module.subprocess.CalledProcessError):self.controller.close_ingress()
  self.assertFalse(self.controller.fence.exists())
  self.assertIsNone(self.controller.record)
 def test_every_legacy_sublocation_must_be_fenced(self):
  guard='if (-f /var/lib/archipelago/app-maintenance/indeedhub) { return 503; }'
  blocks=[f'location /app/indeedhub/{suffix} {{\n {guard}\n proxy_pass http://127.0.0.1:7778/;\n}}' for suffix in ('','_next/','ws/')]
  self.assertEqual(module.validate_nginx_guards('\n'.join(blocks)),3)
  with self.assertRaisesRegex(RuntimeError,'missing its maintenance guard'):module.validate_nginx_guards('\n'.join(blocks).replace(guard,'',1))
  with self.assertRaisesRegex(RuntimeError,'Unrecognized direct'):module.validate_nginx_guards('\n'.join(blocks)+'\nlocation /other/ {\n proxy_pass http://127.0.0.1:7778/;\n}')
 def test_pre_acquire_abort_acknowledges_without_mutating_foreign_fence(self):
  c=self.controller;runtime=c.data/'update-transactions'/'supervised'/(self.operation+'.json')
  module.atomic(runtime,{'phase':'Aborted','target_startup_began':False})
  self.assertEqual(c.release('aborted')['state'],'released')
  c.fence.parent.mkdir(parents=True);foreign=str(uuid.uuid4());c.fence.write_text(foreign)
  self.assertEqual(c.release('aborted')['state'],'released');self.assertEqual(c.fence.read_text(),foreign)
  module.atomic(runtime,{'phase':'Aborted','target_startup_began':True})
  with self.assertRaisesRegex(RuntimeError,'Untouched abort'):c.release('aborted')
 def test_matching_fence_without_journal_is_not_an_untouched_abort(self):
  c=self.controller;module.atomic(c.data/'update-transactions'/'supervised'/(self.operation+'.json'),{'phase':'Aborted','target_startup_began':False})
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  with self.assertRaisesRegex(RuntimeError,'without journal'):c.release('aborted')
  self.assertTrue(c.fence.exists())
 def test_queue_requires_complete_nonnegative_integer_observations(self):
  valid={name:0 for name in module.QUEUE_COUNTS}
  for counts in ({}, {k:v for k,v in valid.items() if k!='active'},
                 dict(valid,active=-1),dict(valid,active=True),dict(valid,unknown=0)):
   with self.subTest(counts=counts):
    self.controller.runner=lambda argv,timeout,output:json.dumps({'paused':True,'counts':counts}).encode()
    with self.assertRaisesRegex(RuntimeError,'queue observation'):self.controller.queue('status')
    self.assertIsNone(self.controller.record)
  self.controller.runner=lambda argv,timeout,output:json.dumps({'paused':True,'counts':valid}).encode()
  self.assertEqual(self.controller.queue('status')['counts'],valid)
 def legacy_forced_fixture(self):
  c=self.controller;unit=self.root/'worker.container';unit.write_text('[Container]\nImage=original\n')
  worker=next(m for m in members() if m['name']=='indeedhub-ffmpeg');worker['unit_sha256']=module.sha(unit)
  counts={name:0 for name in module.QUEUE_COUNTS}
  c.record={'ingress_closed':True,'queue_pause_confirmed':True,'last_queue_counts':counts,
   'stopped':{'indeedhub':{'confirmed':True},worker['name']:{'container_id':worker['container_id'],'intent_at':1700000000}}}
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  original={'name':worker['name'],'container_id':worker['container_id'],'image':worker['image_id'],'config_sha256':worker['config_sha256'],'body':unit.read_text()}
  recovery={'source_container_id':worker['container_id'],'operation_id':self.operation,'image':'d'*64}
  runtime={'id':self.operation,'phase':'Restoring','target_startup_began':False,'members':[{'original':original,'recovery_image':recovery}]}
  runtimepath=c.data/'update-transactions'/'supervised'/(self.operation+'.json');module.atomic(runtimepath,runtime)
  image={'Id':'d'*64,'Created':'2023-01-01T00:00:00Z','Config':{'Cmd':['node','dist/ffmpeg-worker/worker.js'],'Entrypoint':['docker-entrypoint.sh']}}
  observed={'paused':True,'counts':dict(counts)};state={'ps':b''}
  def runner(argv,timeout,output):
   if argv[:3]==['podman','image','inspect']:return json.dumps([image]).encode()
   if argv[:3]==['systemctl','--user','show']:return str(unit).encode()
   if argv[:2]==['podman','ps']:return state['ps']
   if argv[:2]==['podman','exec']:return json.dumps(observed).encode()
   raise AssertionError(argv)
  c.runner=runner
  props='ActiveState=failed\nSubState=failed\nExecMainStatus=137\nResult=exit-code\n'
  return c,worker,props,runtime,runtimepath,image,observed,state,unit
 def test_legacy_forced_idle_proof_is_explicit_and_does_not_claim_graceful_work(self):
  c,w,p,*_=self.legacy_forced_fixture();proof=c.legacy_idle_worker_termination(w,p)
  self.assertEqual(proof['classification'],'legacy-idle-worker-forced-termination')
  self.assertFalse(proof['graceful']);self.assertFalse(proof['completed_work_claim']);self.assertTrue(proof['process_dead'])
 def test_legacy_forced_idle_rejects_missing_active_or_queued_work_before_stop(self):
  c,w,p,*_=self.legacy_forced_fixture();valid=dict(c.record['last_queue_counts'])
  for field in module.QUEUE_COUNTS:
   for bad in ({k:v for k,v in valid.items() if k!=field},dict(valid,**{field:1})):
    c.record['last_queue_counts']=bad
    with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
 def test_legacy_forced_idle_rejects_reopened_missing_or_nonempty_after_queue(self):
  c,w,p,r,rp,i,after,*_=self.legacy_forced_fixture();valid=dict(after['counts'])
  after['paused']=False
  with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
  after['paused']=True
  for field in module.QUEUE_COUNTS:
   for bad in ({k:v for k,v in valid.items() if k!=field},dict(valid,**{field:1})):
    after['counts']=bad
    with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
 def test_legacy_forced_idle_rejects_changed_original_recovery_and_operation(self):
  import copy
  c,w,p,r,rp,*_=self.legacy_forced_fixture()
  mutations=[lambda d:d.update(id='foreign'),lambda d:d.update(target_startup_began=True),lambda d:d.update(phase='Committed')]
  for key in ('container_id','image','config_sha256','body'):
   mutations.append(lambda d,k=key:d['members'][0]['original'].update({k:'changed'}))
  for key in ('source_container_id','operation_id'):
   mutations.append(lambda d,k=key:d['members'][0]['recovery_image'].update({k:'changed'}))
  for mutate in mutations:
   damaged=copy.deepcopy(r);mutate(damaged);module.atomic(rp,damaged)
   with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
 def test_legacy_forced_idle_rejects_unproven_process_command_unit_and_ingress(self):
  c,w,p,r,rp,image,after,state,unit=self.legacy_forced_fixture()
  for field,value in [('Id','e'*64),('Created','2030-01-01T00:00:00Z'),('Config',{'Cmd':['other'],'Entrypoint':['docker-entrypoint.sh']})]:
   old=image[field];image[field]=value
   with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
   image[field]=old
  state['ps']=(w['container_id']+' running').encode()
  with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
  state['ps']=b''
  with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p.replace('ExecMainStatus=137','ExecMainStatus=0'))
  c.record['ingress_closed']=False
  with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
  c.record['ingress_closed']=True;unit.write_text('changed')
  with self.assertRaises(RuntimeError):c.legacy_idle_worker_termination(w,p)
 def test_legacy_worker_sigterm_requires_proven_paused_idle_queue(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared','original_members':module.validate_members(members())};c.save()
  worker=next(m for m in members() if m['name']=='indeedhub-ffmpeg')
  def command(argv,timeout,output):
   if argv[:2]==['podman','inspect']:return self.command_runner(argv,timeout,output)
   if argv[:3]==['systemctl','--user','stop']:return b''
   if argv[:3]==['systemctl','--user','show']:return b'ActiveState=inactive\nResult=success\n'
   if argv[:2]==['podman','events']:return json.dumps({'ID':worker['container_id'],'ContainerExitCode':143}).encode()
   raise AssertionError(argv)
  c.runner=command
  with self.assertRaises(RuntimeError):c.graceful_stop(worker['name'])
  c.record['queue_pause_confirmed']=True;c.record['last_queue_counts']={name:(1 if name=='active' else 0) for name in module.QUEUE_COUNTS}
  with self.assertRaises(RuntimeError):c.graceful_stop(worker['name'])
  c.record['last_queue_counts']['active']=0;c.graceful_stop(worker['name'])
  self.assertEqual(c.record['stopped'][worker['name']]['classification'],'idle-worker-terminated-after-queue-drain')
 def test_legacy_api_compatibility_requires_fresh_empty_business_state(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared','stopped':{'indeedhub':{'confirmed':True},'indeedhub-ffmpeg':{'confirmed':True}},'queue_pause_confirmed':True,'last_queue_counts':{name:0 for name in module.QUEUE_COUNTS}};c.save()
  counts={name:0 for name in ('projects','contents','payments','shareholders','subscriptions','library_items','other_active_transactions')}
  c.runner=lambda argv,timeout,output:json.dumps(counts).encode()
  counts['payments']=1
  with self.assertRaisesRegex(RuntimeError,'business work'):c.legacy_api_idle()
  self.assertNotIn('legacy_api_empty_state',c.record)
  counts['payments']=0;counts['other_active_transactions']=1
  with self.assertRaisesRegex(RuntimeError,'business work'):c.legacy_api_idle()
  counts['other_active_transactions']=0;c.legacy_api_idle()
  self.assertEqual(c.record['legacy_api_empty_state'],counts)
 def test_rollback_compatibility_binds_operation_preserves_rows_and_allows_only_empty_additions(self):
  import copy
  table={'schema':{'columns':['original']},'rows':0,'rows_sha256':'a'*64}
  before={'operation_id':self.operation,'tables':{'migrations':copy.deepcopy(table),'contents':copy.deepcopy(table)},'migrations':[{'id':1,'timestamp':1,'name':'Original1'}]}
  after=copy.deepcopy(before)
  self.assertEqual(module.verify_database_compatibility(before,after)['original_tables'],2)
  names=list(module.ADDITIVE_MIGRATIONS)
  after['migrations'] += [{'id':i+2,'timestamp':module.ADDITIVE_MIGRATIONS[name],'name':name} for i,name in enumerate(names)]
  after['tables']['migrations']['rows']=4;after['tables']['migrations']['rows_sha256']='b'*64
  for name in module.ADDITIVE_TABLES:after['tables'][name]=copy.deepcopy(table)
  self.assertEqual(len(module.verify_database_compatibility(before,after)['new_empty_tables']),5)
  for mutate in [lambda d:d.update(operation_id=str(uuid.uuid4())),lambda d:d['tables']['contents'].update(rows_sha256='c'*64),lambda d:d['tables']['contents']['schema'].update(columns=['changed']),lambda d:d['tables']['archipelago_publications'].update(rows=1),lambda d:d['migrations'][0].update(name='Altered'),lambda d:d['migrations'][-1].update(name='Unreviewed'),lambda d:d['tables'].update(unreviewed=copy.deepcopy(table))]:
   damaged=copy.deepcopy(after);mutate(damaged)
   with self.assertRaisesRegex(RuntimeError,'Data compatibility'):module.verify_database_compatibility(before,damaged)
 def test_verified_rollback_records_operation_proof_before_releasing_fence(self):
  c=self.controller;table={'schema':{},'rows':0,'rows_sha256':'a'*64};baseline={'operation_id':self.operation,'tables':{'migrations':table},'migrations':[]}
  c.record={'operation_id':self.operation,'phase':'Recovering','database_before':baseline};c.save()
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  module.atomic(c.data/'update-transactions'/'supervised'/(self.operation+'.json'),{'phase':'Restored','target_startup_began':True})
  c.database_commitments=lambda:baseline
  self.assertEqual(c.release('restored')['state'],'released')
  self.assertEqual(c.record['recovery_data_verification']['operation_id'],self.operation)
  self.assertEqual(c.record['recovery_data_verification']['before_sha256'],c.record['recovery_data_verification']['after_sha256'])
if __name__=='__main__':unittest.main()
