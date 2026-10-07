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
 def test_forced_original_exit_never_marks_writer_completed(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared','original_members':module.validate_members(members())};c.save()
  def command(argv,timeout,output):
   if argv[:2]==['podman','inspect']:return self.command_runner(argv,timeout,output)
   if argv[:3]==['systemctl','--user','stop']:return b''
   if argv[:3]==['systemctl','--user','show']:return b'ActiveState=inactive\nResult=success\n'
   if argv[:2]==['podman','events']:return json.dumps({'ID':members()[0]['container_id'],'ContainerExitCode':137}).encode()
   raise AssertionError(argv)
  c.runner=command
  with self.assertRaisesRegex(RuntimeError,'did not exit cleanly'):c.graceful_stop('indeedhub')
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
  c.record['queue_pause_confirmed']=True;c.record['last_queue_counts']={'active':1}
  with self.assertRaises(RuntimeError):c.graceful_stop(worker['name'])
  c.record['last_queue_counts']['active']=0;c.graceful_stop(worker['name'])
  self.assertEqual(c.record['stopped'][worker['name']]['classification'],'idle-worker-terminated-after-queue-drain')
 def test_legacy_api_compatibility_requires_fresh_empty_business_state(self):
  c=self.controller;c.record={'operation_id':self.operation,'phase':'Prepared','stopped':{'indeedhub':{'confirmed':True},'indeedhub-ffmpeg':{'confirmed':True}},'queue_pause_confirmed':True,'last_queue_counts':{'active':0}};c.save()
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
