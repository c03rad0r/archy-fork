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
 def test_partial_frontend_recovery_preserves_active_work_without_claiming_a_drain(self):
  c=self.controller;c.record={'operation_id':self.operation,'original_members':module.validate_members(members()),'phase':'Prepared','queue_was_paused':False,'queue_pause_confirmed':True,'last_queue_counts':{name:(1 if name=='active' else 0) for name in module.QUEUE_COUNTS},'stopped':{'indeedhub':{'confirmed':True}}};c.save()
  runtime=c.data/'update-transactions'/'supervised'/(self.operation+'.json');module.atomic(runtime,{'phase':'Restoring','target_startup_began':False})
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation);c.close_ingress=lambda:c.fence_matches()
  self.assertEqual(c.acquire(members(),recovery=True)['state'],'recovering')
  self.assertEqual(c.verify()['state'],'held');self.assertFalse(c.record.get('backup_complete',False))
  self.assertNotIn('indeedhub-ffmpeg',c.record['stopped']);self.assertEqual(self.calls,[])
  # Native restoration must first verify preserved originals and the restarted
  # frontend. The helper never fabricates that terminal native decision.
  with self.assertRaises(RuntimeError):c.release('restored')
  module.atomic(runtime,{'phase':'Restored','target_startup_began':False})
  actions=[]
  def queue(action):actions.append(action);return {'paused':False,'counts':dict(c.record['last_queue_counts'])}
  c.queue=queue
  self.assertEqual(c.release('restored')['state'],'released');self.assertEqual(actions,['resume'])
  self.assertEqual(c.record['last_queue_counts']['active'],1)
  self.assertFalse(c.record.get('backup_complete',False));self.assertEqual(self.calls,[])
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
 def legacy_api_fixture(self):
  c,worker,props,r,rp,image,observed,state,unit=self.legacy_forced_fixture()
  api=next(m for m in members() if m['name']=='indeedhub-api');api['unit_sha256']=module.sha(unit)
  original=r['members'][0]['original'];original.update(name=api['name'],container_id=api['container_id'])
  r['members'][0]['recovery_image']['source_container_id']=api['container_id'];module.atomic(rp,r)
  image['Config']['Cmd']=module.LEGACY_API_CMD;image['Config']['Env']=[]
  observed['extra']={'prioritized':0,'waiting_children':0}
  c.record['original_members']=[api if m['name']==api['name'] else m for m in members()]
  c.record['legacy_api_empty_state']={k:0 for k in ('projects','contents','payments','shareholders','subscriptions','library_items','other_active_transactions')}
  c.record['stopped'][api['name']]={'container_id':api['container_id'],'intent_at':1700000000,'api_signal':{'operation_id':self.operation,'container_id':api['container_id'],'intent_at':1700000001,'acknowledged':True,'prefix':'bull:transcode:','queue_binding':{'container_id':next(x['container_id'] for x in members() if x['name']=='indeedhub-redis'),'image_id':'a'*64,'network_id':'network-id','host':'indeedhub-redis','port':6379}}}
  c.record['stopped'][api['name']]['api_signal'].update(acknowledged_at=1700000002,proof={'parent':{'pid':1,'ppid':0,'starttime':'10','command':'npm run start:prod\0'},'child':{'pid':35,'ppid':1,'starttime':'20','command':'node\0dist/main\0'}},before_queue=json.loads(json.dumps(observed)))
  previous=c.runner
  def runner(argv,timeout,output):
   if argv[:2]==['podman','inspect']:
    row=json.loads(self.command_runner(argv,timeout,output))[0];row['NetworkSettings']={'Networks':{'app':{'NetworkID':'network-id','Aliases':['indeedhub-redis']}}};return json.dumps([row]).encode()
   if argv[:2]==['podman','events']:return b''
   return previous(argv,timeout,output)
  c.runner=runner
  return c,api,'ActiveState=failed\nSubState=failed\nResult=exit-code\nExecMainStatus=1\n',image,observed,state
 def test_api_wrapper_requires_acknowledged_original_signal_and_empty_queue(self):
  c,m,p,image,observed,state=self.legacy_api_fixture()
  proof=c.legacy_api_wrapper_termination(m,p)
  self.assertFalse(proof['graceful']);self.assertFalse(proof['completed_work_claim']);self.assertTrue(proof['process_dead'])
  signal=c.record['stopped'][m['name']]['api_signal']
  for field,value in [('acknowledged',False),('container_id','other'),('operation_id','other'),('intent_at',0)]:
   original=signal[field];signal[field]=value
   with self.assertRaises(RuntimeError):c.legacy_api_wrapper_termination(m,p)
   signal[field]=original
  for field in ('prioritized','waiting_children'):
   observed['extra'][field]=1
   with self.assertRaises(RuntimeError):c.legacy_api_wrapper_termination(m,p)
   observed['extra'][field]=0
  for value in ('137','143','0'):
   with self.assertRaises(RuntimeError):c.legacy_api_wrapper_termination(m,p.replace('ExecMainStatus=1\n','ExecMainStatus='+value+'\n'))
  state['ps']=m['container_id'].encode()
  with self.assertRaises(RuntimeError):c.legacy_api_wrapper_termination(m,p)
 def test_api_queue_observer_rejects_wrong_prefix_missing_counts_and_ambiguous_credentials(self):
  c,m,p,image,observed,state=self.legacy_api_fixture()
  with self.assertRaises(RuntimeError):c.observe_empty_redis_queue(m,'wrong:')
  observed['counts'].pop('active')
  with self.assertRaises(RuntimeError):c.observe_empty_redis_queue(m,'bull:transcode:')
  observed['counts']['active']=0
  for env in (['QUEUE_PASSWORD=one','QUEUE_PASSWORD=two'],['QUEUE_PASSWORD=bad\nframe']):
   image['Config']['Env']=env
   with self.assertRaises(RuntimeError):c.observe_empty_redis_queue(m,'bull:transcode:')
 def test_api_partial_durable_proof_never_substitutes_for_signal_acknowledgement(self):
  import copy
  c,m,p,*_=self.legacy_api_fixture();signal=c.record['stopped'][m['name']]['api_signal'];original=copy.deepcopy(signal)
  for key in ('proof','before_queue','acknowledged_at'):
   signal.clear();signal.update(copy.deepcopy(original));signal.pop(key)
   with self.assertRaises(RuntimeError):c.legacy_api_wrapper_termination(m,p)
  signal.clear();signal.update(original)
  for counts in ({'projects':0},{'unrelated':0},dict.fromkeys(module.BUSINESS_COUNTS,False)):
   c.record['legacy_api_empty_state']=counts
   with self.assertRaises(RuntimeError):c.legacy_api_wrapper_termination(m,p)
 def test_api_queue_endpoint_must_share_exact_original_redis_alias(self):
  c,m,p,image,*_=self.legacy_api_fixture()
  image['Config']['Env']=['QUEUE_HOST=indeedhub-redis','QUEUE_PORT=6379']
  api={'Config':{'Env':list(image['Config']['Env'])},'NetworkSettings':{'Networks':{'app':{'NetworkID':'network-id'}}}}
  self.assertEqual(c.api_queue_binding(m,api,image)['host'],'indeedhub-redis')
  api['NetworkSettings']['Networks']['app']['NetworkID']='different'
  with self.assertRaises(RuntimeError):c.api_queue_binding(m,api,image)
  api['NetworkSettings']['Networks']['app']['NetworkID']='network-id';api['Config']['Env']=['QUEUE_HOST=foreign']
  with self.assertRaises(RuntimeError):c.api_queue_binding(m,api,image)
 def test_api_oom_or_replacement_keeps_termination_unconfirmed(self):
  c,m,p,image,observed,state=self.legacy_api_fixture();runner=c.runner
  c.runner=lambda argv,timeout,output: b'{"Status":"oom"}' if argv[:2]==['podman','events'] else runner(argv,timeout,output)
  with self.assertRaisesRegex(RuntimeError,'OOM'):c.legacy_api_wrapper_termination(m,p)
  c.runner=lambda argv,timeout,output: b'replacement-id' if argv[:2]==['podman','ps'] and any('name=^' in x for x in argv) else runner(argv,timeout,output)
  with self.assertRaisesRegex(RuntimeError,'writer is still running'):c.legacy_api_wrapper_termination(m,p)
 def restart_policy_fixture(self):
  c=self.controller;c.record={'operation_id':self.operation};c.runtime_root=self.root/'runtime';c.runtime_root.mkdir(mode=0o700)
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  path=c.api_restart_override_path();state={'original':'always'}
  def runner(argv,timeout,output):
   self.calls.append(argv)
   if argv==['systemctl','--user','daemon-reload']:return b''
   if argv==['systemctl','--user','show','indeedhub-api.service','--property=Restart','--value']:return ('no' if path.exists() else state['original']).encode()
   raise AssertionError(argv)
  c.runner=runner
  return c,path,state
 def test_owned_restart_override_restores_always_without_rewriting_unit(self):
  c,path,state=self.restart_policy_fixture();c.ensure_api_restart_override()
  self.assertEqual(path.read_bytes(),c.api_restart_override_bytes());self.assertEqual(c.record['api_restart_override']['original_policy'],'always')
  self.assertEqual(path.stat().st_mode&0o777,0o600)
  c.ensure_api_restart_override();c.release_api_restart_override();self.assertFalse(path.exists())
  self.assertTrue(c.record['api_restart_override']['released']);calls=len(self.calls);c.release_api_restart_override();self.assertEqual(len(self.calls),calls)
  self.assertTrue(all('stop' not in argv and 'revert' not in argv for argv in self.calls))
 def test_restart_override_conflict_or_tamper_never_overwrites_or_unlinks(self):
  c,path,state=self.restart_policy_fixture();path.write_text('foreign')
  with self.assertRaises(RuntimeError):c.ensure_api_restart_override()
  self.assertEqual(path.read_text(),'foreign');self.assertEqual(self.calls,[])
  path.unlink();c.ensure_api_restart_override();path.write_text('changed')
  for action in (c.ensure_api_restart_override,c.release_api_restart_override):
   with self.assertRaises(RuntimeError):action()
   self.assertEqual(path.read_text(),'changed')
 def test_restart_override_lost_create_reply_and_reboot_missing_file_are_reverified(self):
  c,path,state=self.restart_policy_fixture();c.ensure_api_restart_override();c.record['api_restart_override'].pop('installed');c.save()
  resumed=module.Controller(c.data,self.operation,0,c.runner);resumed.runtime_root=c.runtime_root;resumed.ensure_api_restart_override()
  self.assertEqual(resumed.record['api_restart_override']['original_policy'],'always')
  path.unlink();resumed.ensure_api_restart_override();self.assertTrue(path.exists())
  self.assertEqual(resumed.record['api_restart_override']['original_policy'],'always')
 def test_restart_policy_mismatch_retains_obligation_after_override_removal(self):
  c,path,state=self.restart_policy_fixture();c.ensure_api_restart_override();state['original']='on-failure'
  with self.assertRaises(RuntimeError):c.release_api_restart_override()
  self.assertFalse(path.exists());self.assertFalse(c.record['api_restart_override']['released']);self.assertTrue(c.fence.exists())
  state['original']='always';c.release_api_restart_override();self.assertTrue(c.record['api_restart_override']['released'])
 def test_restart_override_symlink_or_wrong_mode_is_refused(self):
  c,path,state=self.restart_policy_fixture();c.ensure_api_restart_override();path.chmod(0o644)
  with self.assertRaises(RuntimeError):c.release_api_restart_override()
  path.unlink();other=self.root/'foreign';other.write_text('retain');path.symlink_to(other)
  with self.assertRaises(RuntimeError):c.ensure_api_restart_override()
  with self.assertRaises(RuntimeError):c.release_api_restart_override()
  self.assertEqual(other.read_text(),'retain')
 def test_restart_override_writable_ancestor_is_refused(self):
  c,path,state=self.restart_policy_fixture();c.runtime_root.chmod(0o770)
  with self.assertRaises(RuntimeError):c.ensure_api_restart_override()
  self.assertFalse(path.exists());self.assertEqual(self.calls,[])
 def test_unacknowledged_api_signal_is_never_retried(self):
  c,m,p,*_=self.legacy_api_fixture();c.record['stopped'][m['name']]['api_signal']['acknowledged']=False
  with self.assertRaisesRegex(RuntimeError,'Unacknowledged'):c.signal_legacy_api(m)
  self.assertEqual(self.calls,[])
 def test_actual_node_process_selector_rejects_extra_children_reuse_and_failed_signal(self):
  import subprocess
  harness=r'''const vm=require('vm'),assert=require('assert'),script=JSON.parse(require('fs').readFileSync(0,'utf8'));
function run(change={},action='probe',expected){
 const rows={1:{ppid:0,cmd:'npm run start:prod\0\0',start:'100'},35:{ppid:1,cmd:'node\0dist/main\0',start:'200'},999:{ppid:0,cmd:'node probe\0',start:'300'}};
 for(const [key,value] of Object.entries(change))rows[key]=value;
 let output='',killed=[];
 const fs={readdirSync:()=>Object.keys(rows),readFileSync:p=>{const [,id,file]=p.match(/^\/proc\/(\d+)\/(.*)$/);const r=rows[id];if(!r)throw Error('missing');if(file==='cmdline')return r.cmd;if(file==='status')return `PPid:\t${r.ppid}\n`;return `${id} (node) S `+Array(18).fill('0').join(' ')+' '+r.start+' 0';},writeSync:(_,v)=>{output+=v}};
 vm.runInNewContext(script,{require:n=>{assert.equal(n,'fs');return fs},process:{argv:['node',action,JSON.stringify(expected)],kill:(p,s)=>{if(change.fail)throw Error('signal rejected');killed.push([p,s])}}});
 return {proof:JSON.parse(output),killed};
}
const proof=run().proof;assert.equal(proof.child.starttime,'200');
assert.deepEqual(run({},'signal',proof).killed,[[35,'SIGTERM']]);
assert.throws(()=>run({36:{ppid:1,cmd:'other\0',start:'400'}}));
assert.throws(()=>run({35:{ppid:1,cmd:'node other\0',start:'200'}}));
assert.throws(()=>run({35:{ppid:1,cmd:'node\0dist/main\0',start:'201'}},'signal',proof));
assert.throws(()=>run({1:{ppid:0,cmd:'other\0',start:'100'}}));
console.log('process identity cases passed');'''
  result=subprocess.run(['node','-e',harness],input=json.dumps(module.API_PROCESS_SCRIPT),text=True,capture_output=True)
  self.assertEqual(result.returncode,0,result.stderr)
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
  before={'operation_id':self.operation,'tables':{'typeorm_migrations':copy.deepcopy(table),'contents':copy.deepcopy(table)},'migrations':[{'id':1,'timestamp':1,'name':'Original1'}]}
  after=copy.deepcopy(before)
  self.assertEqual(module.verify_database_compatibility(before,after)['original_tables'],2)
  names=list(module.ADDITIVE_MIGRATIONS)
  after['migrations'] += [{'id':i+2,'timestamp':module.ADDITIVE_MIGRATIONS[name],'name':name} for i,name in enumerate(names)]
  after['tables']['typeorm_migrations']['rows']=4;after['tables']['typeorm_migrations']['rows_sha256']='b'*64
  for name in module.ADDITIVE_TABLES:after['tables'][name]=copy.deepcopy(table)
  self.assertEqual(len(module.verify_database_compatibility(before,after)['new_empty_tables']),5)
  for mutate in [lambda d:d.update(operation_id=str(uuid.uuid4())),lambda d:d['tables']['contents'].update(rows_sha256='c'*64),lambda d:d['tables']['contents']['schema'].update(columns=['changed']),lambda d:d['tables']['archipelago_publications'].update(rows=1),lambda d:d['migrations'][0].update(name='Altered'),lambda d:d['migrations'][-1].update(name='Unreviewed'),lambda d:d['tables'].update(unreviewed=copy.deepcopy(table))]:
   damaged=copy.deepcopy(after);mutate(damaged)
   with self.assertRaisesRegex(RuntimeError,'Data compatibility'):module.verify_database_compatibility(before,damaged)
 def test_migration_history_uses_configured_table_and_never_exempts_unrelated_migrations(self):
  import copy
  self.assertEqual(module.MIGRATION_TABLE,'typeorm_migrations')
  self.assertIn('FROM public.typeorm_migrations m;',module.DB_COMMITMENTS_SQL)
  self.assertNotIn('FROM public.migrations m;',module.DB_COMMITMENTS_SQL)
  table={'schema':{},'rows':0,'rows_sha256':'a'*64}
  before={'operation_id':self.operation,'tables':{'typeorm_migrations':copy.deepcopy(table),'migrations':copy.deepcopy(table)},'migrations':[]}
  after=copy.deepcopy(before);after['tables']['migrations']['rows_sha256']='b'*64
  with self.assertRaisesRegex(RuntimeError,'changed original rows'):module.verify_database_compatibility(before,after)
 def test_database_compatibility_rejects_missing_configured_history(self):
  baseline={'operation_id':self.operation,'tables':{},'migrations':[]}
  with self.assertRaisesRegex(RuntimeError,'lacks configured migration history'):module.verify_database_compatibility(baseline,baseline)
 def test_database_observation_requires_actual_typeorm_history_table(self):
  c=self.controller;table={'table':'migrations','schema':{},'rows':0,'rows_sha256':'a'*64}
  def result(argv,timeout,output):return (json.dumps(table)+'\n'+json.dumps({'migration_rows':[]})+'\n').encode()
  c.runner=result
  with self.assertRaisesRegex(RuntimeError,'observation incomplete'):c.database_commitments()
  table['table']='typeorm_migrations'
  self.assertIn('typeorm_migrations',c.database_commitments()['tables'])
 def test_verified_rollback_records_operation_proof_before_releasing_fence(self):
  c=self.controller;table={'schema':{},'rows':0,'rows_sha256':'a'*64};baseline={'operation_id':self.operation,'tables':{'typeorm_migrations':table},'migrations':[]}
  c.record={'operation_id':self.operation,'phase':'Recovering','database_before':baseline};c.save()
  c.fence.parent.mkdir(parents=True);c.fence.write_text(self.operation)
  module.atomic(c.data/'update-transactions'/'supervised'/(self.operation+'.json'),{'phase':'Restored','target_startup_began':True})
  c.database_commitments=lambda:baseline
  self.assertEqual(c.release('restored')['state'],'released')
  self.assertEqual(c.record['recovery_data_verification']['operation_id'],self.operation)
  self.assertEqual(c.record['recovery_data_verification']['before_sha256'],c.record['recovery_data_verification']['after_sha256'])
 def relay_raw(self, child_pid=35, parent_pid=1, ppid=1, command=None):
  parent='\0'.join(module.LEGACY_RELAY_CMD)+'\0'
  child=command or './nostr-rs-relay\0--db\0/usr/src/app/db\0'
  return f'{parent_pid} 0 100 {parent.encode().hex()}\n{child_pid} {ppid} 200 {child.encode().hex()}\n{module.LEGACY_RELAY_BINARY_SHA256}'
 def test_relay_process_proof_requires_exact_wrapper_child_and_complete_identity(self):
  raw=self.relay_raw();proof=module.relay_process_proof(raw,'/usr/src/app/db')
  self.assertEqual(proof['child']['pid'],35)
  for bad in [raw+'\n'+raw.splitlines()[1],self.relay_raw(child_pid=1),self.relay_raw(parent_pid=2),self.relay_raw(ppid=0),self.relay_raw(command='./other\0'),raw.replace(' 200 ',' invalid '),raw.replace(module.LEGACY_RELAY_BINARY_SHA256,'a'*64)]:
   with self.assertRaises((RuntimeError,ValueError)):module.relay_process_proof(bad,'/usr/src/app/db')
  with self.assertRaises(RuntimeError):module.relay_process_proof(raw,'/other')
 def test_relay_override_has_separate_owned_path_and_journal_key(self):
  c,path,state=self.restart_policy_fixture();relay=c.api_restart_override_path('relay');old=c.runner
  def runner(argv,timeout,output):
   if argv==['systemctl','--user','show','indeedhub-relay.service','--property=Restart','--value']:return ('no' if relay.exists() else 'always').encode()
   return old(argv,timeout,output)
  c.runner=runner;c.ensure_api_restart_override('relay')
  self.assertTrue(relay.exists());self.assertFalse(path.exists());self.assertNotIn('api_restart_override',c.record)
  self.assertEqual(c.record['relay_restart_override']['original_policy'],'always')
  c.ensure_api_restart_override();c.release_api_restart_override('relay')
  self.assertTrue(path.exists());self.assertFalse(relay.exists());self.assertFalse(c.record['api_restart_override']['released'])
  with self.assertRaises(RuntimeError):c.api_restart_override_path('postgres')
 def test_relay_unacknowledged_or_changed_durable_signal_never_retries(self):
  c=self.controller;m=next(x for x in members() if x['name']=='indeedhub-relay')
  raw=self.relay_raw();saved={'operation_id':self.operation,'container_id':m['container_id'],'signal':'SIGINT','proof':module.relay_process_proof(raw,'/usr/src/app/db'),'raw':raw,'intent_at':1700000001,'acknowledged_at':1700000002,'acknowledged':True}
  c.record={'stopped':{m['name']:{'intent_at':1700000000,'relay_signal':saved}}}
  c.api_recovery_identity=lambda member,role:{'Created':'2020-01-01T00:00:00Z','Config':{'Env':['APP_DATA=/usr/src/app/db']}}
  c.signal_legacy_relay(m);self.assertEqual(self.calls,[])
  for key,bad in [('acknowledged',False),('operation_id',str(uuid.uuid4())),('container_id','wrong'),('proof',{}),('signal','SIGTERM'),('acknowledged_at',1)]:
   old=saved[key];saved[key]=bad
   with self.assertRaises(RuntimeError):c.signal_legacy_relay(m)
   saved[key]=old
  self.assertEqual(self.calls,[])
 def test_acknowledged_api_and_relay_retry_refuses_replacement_before_stop(self):
  for name in ('indeedhub-api','indeedhub-relay'):
   c=self.controller;m=next(x for x in members() if x['name']==name)
   c.record={'original_members':members(),'stopped':{name:{'container_id':m['container_id'],'intent_at':1700000000}}}
   c.signal_legacy_api=lambda member:None;c.signal_legacy_relay=lambda member:None
   calls=[]
   def runner(argv,timeout,output):
    calls.append(argv)
    if argv[:2]==['podman','ps']:return b'unexpected-replacement'
    raise AssertionError('Replacement must be refused before any stop '+str(argv))
   c.runner=runner
   with self.assertRaisesRegex(RuntimeError,'writer is still running'):c.graceful_stop(name)
   self.assertEqual(len(calls),1);self.assertEqual(calls[0][:2],['podman','ps'])
 def test_relay_image_requires_qualified_base_or_completed_owned_recovery_lineage(self):
  import copy,hashlib
  image='d'*64;body='[Container]\nImage='+image+'\n';before='[Container]\nImage='+module.LEGACY_RELAY_IMAGE+'\n'
  record={'schema':2,'target_startup_began':False,'id':self.operation,'package':'indeedhub','phase':'Restored','cleanup_done':True,'members':[{'original':{'name':'indeedhub-relay','image':module.LEGACY_RELAY_IMAGE,'body':before,'container_id':'e'*64},'pinned_original_body':body,'preserve_original':False,'recovery_image':{'image':image,'operation_id':self.operation,'source_container_id':'e'*64}}]}
  digest=hashlib.sha256(body.encode()).hexdigest()
  module.verify_relay_image_lineage(module.LEGACY_RELAY_IMAGE,'unused',[])
  installed={'schema':1,'name':'indeedhub-relay','operation':self.operation,'body':body}
  module.verify_relay_image_lineage(image,digest,[record],installed)
  for missing in (None,dict(installed,operation=str(uuid.uuid4())),dict(installed,body='changed')):
   with self.assertRaises(RuntimeError):module.verify_relay_image_lineage(image,digest,[record],missing)
  for bad in [[],[record,record]]:
   with self.assertRaises(RuntimeError):module.verify_relay_image_lineage(image,digest,bad,installed)
  post_target=copy.deepcopy(record);post_target['target_startup_began']=True;post_target['members'][0]['preserve_original']=None
  module.verify_relay_image_lineage(image,digest,[post_target],installed)
  for started,preserved in ((True,False),(False,None),(0,False),(1,None),(None,False)):
   bad=copy.deepcopy(record);bad['target_startup_began']=started;bad['members'][0]['preserve_original']=preserved
   with self.assertRaises(RuntimeError):module.verify_relay_image_lineage(image,digest,[bad],installed)
  missing=copy.deepcopy(record);missing.pop('target_startup_began')
  with self.assertRaises(RuntimeError):module.verify_relay_image_lineage(image,digest,[missing],installed)
  legacy=copy.deepcopy(record);legacy['schema']=1
  for malformed in (0,'false',[],{}):
   legacy['members'][0]['preserve_original']=malformed
   with self.assertRaises(RuntimeError):module.verify_relay_image_lineage(image,digest,[legacy],installed)
  for change in ('unfinished','foreign','preserved','wrong-body','cycle'):
   bad=copy.deepcopy(record)
   if change=='unfinished':bad['cleanup_done']=False
   if change=='foreign':bad['members'][0]['recovery_image']['operation_id']=str(uuid.uuid4())
   if change=='preserved':bad['members'][0]['preserve_original']=True
   if change=='wrong-body':bad['members'][0]['pinned_original_body']='changed'
   if change=='cycle':bad['members'][0]['original']['image']=image
   with self.assertRaises(RuntimeError):module.verify_relay_image_lineage(image,digest,[bad],installed)
 def test_column_commitments_retain_logical_order_and_every_semantic_field(self):
  import copy
  columns=[['first','integer',True,'','',''],['last','text',False,'','','']]
  before={'operation_id':self.operation,'tables':{'typeorm_migrations':{'schema':{},'rows':0,'rows_sha256':'a'},'items':{'schema':{'columns':columns},'rows':0,'rows_sha256':'b'}},'migrations':[]}
  module.verify_database_compatibility(before,copy.deepcopy(before))
  for replacement in (list(reversed(columns)),columns[:-1],[['first','bigint',True,'','',''],columns[1]]):
   after=copy.deepcopy(before);after['tables']['items']['schema']['columns']=replacement
   with self.assertRaisesRegex(RuntimeError,'changed original table schema'):module.verify_database_compatibility(before,after)
 def test_failure_diagnostics_are_private_bounded_and_do_not_change_journal(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared'};c.save();before=c.path.read_bytes()
  c.save_failure('acquire',RuntimeError('diagnostic'*1000));path=c.root/'last-failure.private.json';record=json.loads(path.read_text())
  self.assertEqual(record['operation_id'],self.operation);self.assertEqual(record['action'],'acquire');self.assertEqual(record['error_type'],'RuntimeError');self.assertEqual(len(record['message']),4096)
  self.assertEqual(path.stat().st_mode&0o777,0o600);self.assertEqual(c.path.read_bytes(),before)
if __name__=='__main__':unittest.main()
