#!/usr/bin/env python3
"""Exercise the production Bump RPC against disposable Bitcoin/LND regtest wallets."""
import http.cookiejar
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import urllib.error
import urllib.request

assert os.environ.get('ARCHY_FEE_REGTEST_ISOLATED') == '1'
assert os.getpid() == 1, 'A private PID namespace is required'
assert os.readlink('/proc/self/ns/net') != os.environ['ARCHY_HOST_NET_NS'], 'Host network refused'
BIN = Path('/opt/archy-regtest-bin')
ROOT = Path('/var/lib/archipelago')
RESULTS = Path('/opt/archy-regtest-results')
ROOT.mkdir(parents=True, exist_ok=True)
RESULTS.mkdir(parents=True, exist_ok=True)
procs = []
logs = []
def launch(name, args, env=None):
    log = (RESULTS / (name + '.log')).open('ab'); logs.append(log)
    proc = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT, env=env)
    procs.append(proc)
    return proc

def wait(check, seconds=90):
    until = time.monotonic() + seconds
    last = None
    while time.monotonic() < until:
        try:
            result = check()
            if result: return result
        except Exception as error: last = error
        time.sleep(.5)
    raise RuntimeError('Regtest readiness timeout: ' + str(last))

def cli(args):
    proc = subprocess.run(args, capture_output=True, text=True, timeout=20)
    if proc.returncode: raise RuntimeError(proc.stderr.strip()[:300])
    try: return json.loads(proc.stdout)
    except json.JSONDecodeError: return proc.stdout.strip()

bitcoin_dir = ROOT / 'regtest-bitcoin'
bitcoin_dir.mkdir()
password = secrets.token_hex(24)
conf = bitcoin_dir / 'bitcoin.conf'
conf.write_text('regtest=1\nserver=1\ndbcache=64\nlisten=0\ndiscover=0\ndnsseed=0\nfallbackfee=0.00002\n'
                '[regtest]\nrpcbind=127.0.0.1\nrpcallowip=127.0.0.1\nrpcport=8332\n'
                'rpcuser=archipelago\nrpcpassword=' + password + '\n'
                'zmqpubrawblock=tcp://127.0.0.1:28332\nzmqpubrawtx=tcp://127.0.0.1:28333\n')
conf.chmod(0o600)
secrets_dir = ROOT / 'secrets'; secrets_dir.mkdir()
(secrets_dir / 'bitcoin-rpc-password').write_text(password)
(secrets_dir / 'bitcoin-rpc-password').chmod(0o600)
def btc(method, *args):
    return cli([str(BIN/'bitcoin-cli'), '-datadir='+str(bitcoin_dir), method, *[str(a) for a in args]])
def ln(method, *args):
    return cli([str(BIN/'lncli'), '--lnddir='+str(ROOT/'lnd'), '--network=regtest', method, *[str(a) for a in args]])
jar = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
def rpc(method, params=None):
    headers = {'Content-Type':'application/json'}
    csrf = next((c.value for c in jar if c.name == 'csrf_token'), None)
    if csrf: headers['X-CSRF-Token'] = csrf
    req = urllib.request.Request('http://127.0.0.1:5678/rpc/v1',
        data=json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params or {}}).encode(), headers=headers)
    with opener.open(req, timeout=60) as response: data = json.load(response)
    if data.get('error'): raise RuntimeError(data['error'].get('message', 'RPC error'))
    return data['result']

try:
    launch('bitcoin', [str(BIN/'bitcoind'), '-datadir='+str(bitcoin_dir)])
    wait(lambda: btc('getblockchaininfo')['chain'] == 'regtest')
    btc('createwallet', 'miner')
    miner = btc('getnewaddress')
    btc('generatetoaddress', 101, miner)
    lnd_dir = ROOT/'lnd'; lnd_dir.mkdir()
    lnd_conf = lnd_dir/'lnd.conf'
    lnd_conf.write_text('[Application Options]\nnoseedbackup=1\nrestlisten=127.0.0.1:18080\nrpclisten=127.0.0.1:10009\nlisten=127.0.0.1:9735\n'
        '[Bitcoin]\nbitcoin.active=1\nbitcoin.regtest=1\nbitcoin.node=bitcoind\n'
        '[Bitcoind]\nbitcoind.rpchost=127.0.0.1:8332\nbitcoind.rpcuser=archipelago\nbitcoind.rpcpass='+password+'\n'
        'bitcoind.zmqpubrawblock=tcp://127.0.0.1:28332\nbitcoind.zmqpubrawtx=tcp://127.0.0.1:28333\n')
    lnd_conf.chmod(0o600)
    launch('lnd', [str(BIN/'lnd'), '--lnddir='+str(lnd_dir)])
    wait(lambda: ln('getinfo')['synced_to_chain'], 120)
    # The production backend uses this canonical pathname. Only the namespace's
    # disposable filesystem is changed; the host's real wallet is inaccessible.
    (lnd_dir/'data/chain/bitcoin/mainnet').symlink_to('regtest')
    address = ln('newaddress','p2wkh')['address']
    btc('sendtoaddress', address, '0.01'); btc('generatetoaddress', 6, miner)
    wait(lambda: int(ln('walletbalance')['confirmed_balance']) >= 1_000_000)
    env = dict(os.environ, ARCHIPELAGO_DATA_DIR=str(ROOT), ARCHIPELAGO_BIND='127.0.0.1:5678',
               ARCHIPELAGO_APPS_DIR=str(ROOT/'empty-apps'), ARCHIPELAGO_LOG_LEVEL='warn')
    (ROOT/'empty-apps').mkdir()
    backend = launch('backend', [str(BIN/'archipelago')], env)
    wait(lambda: urllib.request.urlopen('http://127.0.0.1:5678/health',timeout=3).status == 200, 180)
    account_password = secrets.token_urlsafe(24)
    rpc('auth.setup', {'password':account_password})
    rpc('auth.login', {'password':account_password})
    assert rpc('system.get-hostname')
    recipient = btc('getnewaddress')
    sent = ln('sendcoins', '--addr='+recipient, '--amt=100000', '--sat_per_vbyte=1')
    txid = sent['txid']
    wait(lambda: btc('getmempoolentry', txid))
    quote = rpc('lnd.bump-quote', {'txid':txid, 'sat_per_vbyte':5})
    assert quote['method'] == 'cpfp' and quote['recipient_sats'] == 100000
    assert 0 < quote['budget_sats'] < quote['input_sats']
    result = rpc('lnd.bump-submit', {'txid':txid, 'quote_id':quote['quote_id']})
    assert result['status'] in ['registered','mempool']
    status = wait(lambda: (s if (s:=rpc('lnd.bump-status',{'txid':txid}))['status']=='mempool' else None),90)
    child = status['bump_txid']
    raw_parent=btc('getrawtransaction',txid,'true')
    assert any(o['scriptPubKey'].get('address')==recipient and round(o['value']*100_000_000)==100000 for o in raw_parent['vout'])
    fee = int(status['actual_sweep_fee_sats'])
    assert 0 < fee <= quote['budget_sats']
    duplicate=rpc('lnd.bump-submit',{'txid':txid,'quote_id':quote['quote_id']})
    assert duplicate['bump_txid']==child
    print('PASS real regtest CPFP: quote, explicit budget, broadcast, mempool, recipient preserved, duplicate submission', flush=True)
    replacement_quote = wait(lambda: rpc('lnd.bump-quote', {'txid':child, 'sat_per_vbyte':10}), 30)
    assert replacement_quote['method'] == 'rbf'
    assert replacement_quote['recipient_sats'] == 100000
    old_child = child
    rpc('lnd.bump-submit', {'txid':old_child, 'quote_id':replacement_quote['quote_id']})
    replaced = wait(lambda: (s if (s:=rpc('lnd.bump-status', {'txid':old_child}))['status']=='mempool' else None), 90)
    child = replaced['bump_txid']
    assert child != old_child and int(replaced['actual_sweep_fee_sats']) <= replacement_quote['budget_sats']
    assert old_child not in btc('getrawmempool')
    assert rpc('lnd.bump-status', {'txid':txid})['bump_txid'] == child
    print('PASS real regtest RBF of CPFP child and original operation tracks replacement', flush=True)
    def verify_history():
        rows = rpc('lnd.gettransactions')['transactions']
        parent = next(t for t in rows if t['tx_hash'] == txid)
        assert parent['amount_sats'] - parent['total_fees'] == 100000
        assert parent['fee_bump_txid'] == child
        assert parent['bump_fee_sats'] == int(replaced['actual_sweep_fee_sats'])
        assert not any(t['tx_hash'] in [child, old_child] for t in rows)
        assert sum(h['status'] != 'replaced' for h in parent['fee_bump_history']) == 1
        return True
    wait(verify_history)
    print('PASS one payment with verified fee history; replacement fee not double-counted', flush=True)

    backend.terminate(); backend.wait(timeout=25)
    backend=launch('backend',[str(BIN/'archipelago')],env)
    wait(lambda: urllib.request.urlopen('http://127.0.0.1:5678/health',timeout=3).status == 200,180)
    rpc('auth.login',{'password':account_password})
    restored=rpc('lnd.bump-status',{'txid':txid})
    assert restored['bump_txid']==child and restored['status']=='mempool'
    wait(verify_history)
    print('PASS durable operation and grouped history after actual backend restart',flush=True)
    block=btc('generatetoaddress',1,miner)[0]
    wait(lambda: rpc('lnd.bump-status',{'txid':txid})['status']=='confirmed')
    btc('invalidateblock',block)
    # A competing empty block announces the alternative chain to LND's ZMQ
    # backend while leaving the payment package unconfirmed in Bitcoin.
    btc('generateblock', miner, '[]')
    try:
        wait(lambda: rpc('lnd.bump-status',{'txid':txid})['status']=='mempool')
    except RuntimeError:
        diagnostic = {'bump':rpc('lnd.bump-status',{'txid':txid}), 'mempool':btc('getrawmempool'),
                      'lnd_history':ln('listchaintxns'), 'lnd_info':ln('getinfo')}
        (RESULTS/'reorg.json').write_text(json.dumps(diagnostic,indent=2))
        print('Reorg diagnostic:', diagnostic['bump']['status'], 'mempool size', len(diagnostic['mempool']),flush=True)
        raise
    wait(verify_history)
    print('PASS confirmation and reorg return to mempool with grouped history',flush=True)
    (RESULTS/'result.json').write_text(json.dumps({'passed':True,'network':'regtest','real_funds_used':False}))
except Exception:
    # Disposable regtest data only; retain enough evidence to diagnose a failed
    # relationship/chain-state assertion without weakening the acceptance test.
    diagnostic = {}
    for name, call in [('wallet_history', lambda: ln('listchaintxns')),
                       ('normalized_history', lambda: rpc('lnd.gettransactions'))]:
        try: diagnostic[name] = call()
        except Exception as error: diagnostic[name] = str(error)
    (RESULTS/'failure-history.json').write_text(json.dumps(diagnostic, indent=2))
    raise
finally:
    for proc in reversed(procs):
        if proc.poll() is None:
            proc.terminate()
            try: proc.wait(timeout=20)
            except subprocess.TimeoutExpired: proc.kill(); proc.wait()
    for log in logs: log.close()
