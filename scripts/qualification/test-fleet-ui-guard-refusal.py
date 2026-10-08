import json,pathlib,tempfile,runpy,unittest,sys
from unittest.mock import patch
class GuardRefusal(unittest.TestCase):
 def test_backend_gate_refuses_before_any_service_or_file_mutation(self):
  good={'host':'archi-dev-box','isolated_failed':0,'isolated_passed':2011,'production_build_passed':True,'source_inputs_unchanged':True,'test_source_manifest_sha256':'a'*64,'build_source_manifest_sha256':'a'*64,'fleet_provenance_tests_passed':['unsigned_and_legacy_reports_cannot_assign_their_own_trust','collector_cannot_claim_another_record_identity_or_malformed_id','collector_history_suffix_cannot_overwrite_another_nodes_history','fleet_reads_do_not_promote_old_collector_spoofs_or_history'],'contains_collector_provenance':True,'fleet_contract_readonly_checks_passed':True,'deployment_preservation_passed':True,'embedded_helpers_match':True}
  q={'baseline_tests_passed':1459,'baseline_test_files_passed':177,'delta_focused_tests_passed':31,'delta_app_typecheck_passed':True,'production_build_passed':True,'source_inputs_unchanged':True,'archive_sha256':'063752912fdcae2d1abf2abaccd53bfad39acce23c9a5d896ce52ba1e1996080'}
  cases=[{'isolated_failed':1},{'build_source_manifest_sha256':'b'*64},{'fleet_provenance_tests_passed':[]},{'fleet_contract_readonly_checks_passed':False},{'embedded_helpers_match':False}]
  with tempfile.TemporaryDirectory() as d:
   r=pathlib.Path(d);(r/'before').write_text(json.dumps({'host':'archi-dev-box'}));(r/'q').write_text(json.dumps(q))
   for delta in cases:
    with self.subTest(delta=delta):
     (r/'backend').write_text(json.dumps(good|delta))
     def check(args,**kwargs):
      if args==['hostname']:return 'archi-dev-box\n'
      raise RuntimeError('Unexpected live command before refusal')
     with patch('subprocess.check_output',side_effect=check),patch('subprocess.run',side_effect=RuntimeError('Mutation attempted')) as mutation,patch.object(sys,'argv',['guard','deploy','archi-dev-box',str(r/'before'),str(r/'q'),str(r/'archive'),str(r/'backend')]):
      with self.assertRaises(AssertionError):runpy.run_path(str(pathlib.Path(__file__).with_name('fleet-ui-guard.py')),run_name='__main__')
      mutation.assert_not_called()
unittest.main()
