import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch, MagicMock
import tempfile
import json

spec=importlib.util.spec_from_file_location('repair', Path(__file__).parents[1]/'repair-npm-tunnel.py')
m=importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
DROP='[Container]\nPublishPort=10.77.0.2:18080:80/tcp\nPublishPort=10.77.0.2:18443:443/tcp\n'
RULES='''table inet web_tunnel {
    chain input {
        type filter hook input priority -10; policy accept;
        iifname != "wg-web" return
        ct state established,related accept
        ip saddr 10.77.0.1 icmp type echo-request accept
        ip saddr 10.77.0.1 ip daddr 10.77.0.2 tcp dport { 18080, 18443 } accept
        counter drop
    }
    chain forward {
        type filter hook forward priority -10; policy accept;
        iifname "wg-web" counter drop
        oifname "wg-web" counter drop
    }
}'''

class Plan(unittest.TestCase):
    def test_preserves_peer_https_and_restricts_redirect(self):
        drop,rules,dst=m.plan(DROP,RULES)
        self.assertEqual(dst,'10.77.0.2')
        self.assertIn('PublishPort=10.77.0.2:18081:80/tcp',drop)
        self.assertIn('PublishPort=10.77.0.2:18443:443/tcp',drop)
        self.assertIn('iifname "wg-web" ip saddr 10.77.0.1 ip daddr 10.77.0.2 tcp dport 18080 redirect to :18081',rules)
        self.assertNotIn('tcp dport { 18080, 18443 } accept',rules)
        self.assertIn('iifname "wg-web" counter drop',rules)
    def test_idempotent(self):
        d,r,_=m.plan(DROP,RULES)
        self.assertIsNone(m.plan(d,r))
    def test_standard_fresh_install_untouched(self):
        self.assertIsNone(m.plan('[Container]\nPublishPort=127.0.0.1:8081:81/tcp',''))
    def test_no_hardcoded_deployment_address(self):
        d,r,dst=m.plan(DROP.replace('10.77.0.','10.55.0.'),RULES.replace('10.77.0.','10.55.0.'))
        self.assertEqual(dst,'10.55.0.2');self.assertIn('ip saddr 10.55.0.1',r)
    def test_custom_firewall_preserved(self):
        for r in [RULES+'\ntable inet extra {}',RULES.replace('counter drop','accept'),RULES.replace('wg-web','wg-custom')]:
            with self.assertRaises(ValueError): m.plan(DROP,r)
    def test_ambiguous_mapping(self):
        with self.assertRaises(ValueError): m.plan(DROP+DROP,RULES)
    def test_wrong_destination(self):
        with self.assertRaises(ValueError): m.plan(DROP.replace('10.77.0.2','10.77.0.3'),RULES)
    def test_already_used_mapping(self):
        with self.assertRaises(ValueError): m.plan(DROP+'PublishPort=10.77.0.2:18081:80/tcp\n',RULES)

class Migration(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name)
        self.drop=self.home/'.config/containers/systemd/nginx-proxy-manager.container.d/web-tunnel.conf'
        self.drop.parent.mkdir(parents=True)
        self.drop.write_text(DROP)
        self.calls=[];self.rules=RULES;self.fail=None
        self.state=self.home/'.local/state/archipelago/npm-tunnel-migration'
    def command(self,*args,input=None):
        self.calls.append((args,input))
        if self.fail and self.fail(args):
            self.fail=None
            raise RuntimeError('injected failure')
        if 'cat' in args or ('list' in args and 'nft' in args): return self.rules
        if 'grep' in args: return 'PreUp = nft -f /etc/wireguard/wg-web.nft\nPostDown = nft delete table inet web_tunnel'
        if 'show' in args: return 'active'
        return ''
    def run_migration(self):
        with patch.object(Path,'home',return_value=self.home),patch.object(m,'command',side_effect=self.command),patch.object(m,'root_write') as write,patch.object(m.socket,'socket'):
            m.main()
            return write
    def test_success_and_second_run_noop(self):
        write=self.run_migration()
        self.assertIn(':18081:80/tcp',self.drop.read_text())
        self.assertEqual(json.loads((self.state/'before.json').read_text())['drop'],DROP)
        self.assertFalse((self.state/'pending.json').exists())
        self.assertEqual(write.call_count,1)
        self.calls.clear();self.run_migration();self.assertEqual(self.calls,[])
    def test_no_native_service_commands(self):
        self.run_migration()
        for args,_ in self.calls:
            self.assertNotIn('lnd.service',args);self.assertNotIn('bitcoin-core.service',args)
    def test_validation_failure_does_not_stop_or_write(self):
        self.fail=lambda a:'--check' in a
        with self.assertRaises(RuntimeError):self.run_migration()
        self.assertEqual(self.drop.read_text(),DROP)
        self.assertFalse(self.state.exists())
        self.assertFalse(any('stop' in a for a,_ in self.calls))
    def test_apply_failure_restores_files_and_firewall(self):
        self.fail=lambda a:'nft' in a and '-f' in a and '--check' not in a
        with self.assertRaises(RuntimeError):self.run_migration()
        self.assertEqual(self.drop.read_text(),DROP)
        self.assertFalse((self.state/'pending.json').exists())
        self.assertTrue(any(v=='delete table inet web_tunnel\n'+RULES for _,v in self.calls))
    def test_crash_journal_recovers_and_retries(self):
        self.state.mkdir(parents=True)
        (self.state/'pending.json').write_text(json.dumps({'drop':DROP,'rules':RULES,'was_active':'active'}))
        self.drop.write_text(m.plan(DROP,RULES)[0])
        self.run_migration()
        self.assertIn(':18081:80/tcp',self.drop.read_text())
        self.assertFalse((self.state/'pending.json').exists())
    def test_fresh_install_executes_no_commands(self):
        self.drop.unlink();self.run_migration();self.assertEqual(self.calls,[])
    def test_busy_replacement_port_does_not_mutate(self):
        with patch.object(Path,'home',return_value=self.home),patch.object(m,'command',side_effect=self.command),patch.object(m.socket,'socket') as socket:
            socket.return_value.__enter__.return_value.bind.side_effect=OSError('in use')
            with self.assertRaises(OSError):m.main()
        self.assertFalse(self.state.exists())
        self.assertFalse(any('stop' in a for a,_ in self.calls))
    def test_failed_rollback_keeps_recovery_journal(self):
        with patch.object(Path,'home',return_value=self.home),patch.object(m,'command',side_effect=self.command),patch.object(m,'root_write',side_effect=RuntimeError('write failed')),patch.object(m.socket,'socket'):
            with self.assertRaises(RuntimeError):m.main()
        self.assertTrue((self.state/'pending.json').exists())
        self.assertEqual(self.drop.read_text(),DROP)
    def test_stopped_app_is_not_started(self):
        original=self.command
        def stopped(*args,input=None):
            return 'inactive' if 'show' in args else original(*args,input=input)
        with patch.object(Path,'home',return_value=self.home),patch.object(m,'command',side_effect=stopped),patch.object(m,'root_write'),patch.object(m.socket,'socket'):
            m.main()
        self.assertFalse(any('restart' in a for a,_ in self.calls))
    def test_live_only_firewall_changes_are_not_discarded(self):
        original=self.command
        def different(*args,input=None):
            result=original(*args,input=input)
            return result+' table inet custom {}' if 'list' in args else result
        with patch.object(Path,'home',return_value=self.home),patch.object(m,'command',side_effect=different),patch.object(m.socket,'socket'):
            with self.assertRaises(ValueError):m.main()
        self.assertEqual(self.drop.read_text(),DROP)

if __name__=='__main__': unittest.main()
