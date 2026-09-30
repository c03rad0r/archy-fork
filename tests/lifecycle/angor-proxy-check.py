#!/usr/bin/env python3
"""Opt-in disposable rootless Angor gateway integration checks. No native app changes."""
import subprocess,pathlib,json,urllib.request,urllib.error,time,tempfile,os,uuid,shlex
import yaml
if os.environ.get('ARCHY_ALLOW_DISPOSABLE_CONTAINERS') != '1':
 raise SystemExit('Set ARCHY_ALLOW_DISPOSABLE_CONTAINERS=1 to run isolated test containers')
manifest=yaml.safe_load((pathlib.Path(__file__).resolve().parents[2]/'apps/angor-indexer/manifest.yml').read_text())['app']
health=manifest['health_check']
health_url=health['endpoint'].rstrip('/')+health.get('path','/')
run_id=uuid.uuid4().hex[:12]
net='archy-angor-test-'+run_id;backend='angor-test-backend-'+run_id;gateway='angor-test-gateway-'+run_id
def run(*a):
 r=subprocess.run(a,capture_output=True,text=True)
 if r.returncode:raise RuntimeError(r.stderr)
 return r.stdout.strip()
def req(path,data=None,method=None,headers={}):
 r=urllib.request.Request('http://127.0.0.1:19098'+path,data=data,method=method,headers=headers)
 try:
  with urllib.request.urlopen(r,timeout=10) as f:return f.status,f.headers,f.read()
 except urllib.error.HTTPError as e:return e.code,e.headers,e.read()
script="""require('http').createServer((q,r)=>{let b='';q.on('data',x=>b+=x);q.on('end',()=>{r.setHeader('Access-Control-Allow-Origin','https://wrong.example');if(q.url==='/api/v1/blocks/tip/height'){r.end('900000');return}r.setHeader('Content-Type','application/json');r.end(JSON.stringify({url:q.url,method:q.method,body:b,cookie:q.headers.cookie||null,auth:q.headers.authorization||null}))})}).listen(8999,'0.0.0.0')"""
def start_backend():run('podman','run','-d','--name',backend,'--network',net,'--network-alias','mempool-api','--cap-drop=all','--security-opt=no-new-privileges','docker.io/library/node:24-alpine','node','-e',script)
def ready(seconds=40):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  try:
   if req('/health')[0]==200:return
  except OSError:pass
  time.sleep(1)
 raise RuntimeError('Gateway readiness did not recover')
assert subprocess.run(['podman','network','exists',net]).returncode==1
run('podman','network','create',net)
try:
 start_backend()
 run('podman','run','-d','--name',gateway,'--network',net,'--read-only','--cap-drop=all','--security-opt=no-new-privileges','--memory','128m','--health-cmd','wget -q -T 5 -O /dev/null '+shlex.quote(health_url),'--health-interval','5s','--health-retries','2','-p','127.0.0.1:19098:8080','source.archipelago-foundation.org/chaum/angor-indexer:1.0.1')
 ready()
 run('podman','healthcheck','run',gateway)
 assert json.loads(run('podman','inspect',gateway))[0]['State']['Health']['Status']=='healthy'
 print('PASS manifest health check inside actual image (including localhost address family)',flush=True)
 for path in ['/api/v1/address/bc1fixture/txs?after_txid=abc','/api/v1/fees/recommended','/api/tx/fixture/hex']:
  status,headers,body=req(path,headers={'Cookie':'node-secret=do-not-forward','Authorization':'Bearer do-not-forward'})
  result=json.loads(body);assert status==200 and result['url']==(path if path.startswith('/api/v1/') else path.replace('/api/','/api/v1/',1)) and result['cookie'] is None and result['auth'] is None
  assert headers.get_all('Access-Control-Allow-Origin')==['*']
 assert req('/api/v1/tx',b'deadbeef')[0]==200
 assert json.loads(req('/api/v1/tx',b'deadbeef')[2])['body']=='deadbeef'
 assert req('/api/v1/fees/recommended',b'bad')[0]==403
 assert req('/api/v1/tx',b'bad',method='DELETE')[0]==403
 assert req('/api/v1/tx',method='OPTIONS')[0]==204
 assert req('/api/v1/tx',b'x'*(4*1024*1024+1))[0]==413
 assert req('/unknown')[0]==404
 d=json.loads(run('podman','inspect',gateway))[0];assert d['Config']['User']=='101:101' and not d['BoundingCaps']
 print('PASS API paths/query/body, transaction-only POST, method/size limits, CORS, credential stripping and unprivileged read-only image',flush=True)
 run('podman','stop',backend)
 status,headers,body=req('/health');assert status==503 and json.loads(body)['status']=='waiting'
 run('podman','rm',backend);start_backend();ready()
 print('PASS backend outage returns truthful 503; backend recreation recovers through runtime DNS without gateway restart',flush=True)
except BaseException:
 subprocess.run(['podman','logs','--tail','15',gateway],check=False)
 raise
finally:
 for name in [gateway,backend]:subprocess.run(['podman','rm','-f','--time','3',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 subprocess.run(['podman','network','rm',net],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
