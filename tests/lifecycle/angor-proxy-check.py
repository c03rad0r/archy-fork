#!/usr/bin/env python3
"""Opt-in disposable rootless Angor gateway integration checks. No native app changes."""
import subprocess,pathlib,json,urllib.request,urllib.error,time,tempfile,os,uuid,shlex,socket
import yaml
if os.environ.get('ARCHY_ALLOW_DISPOSABLE_CONTAINERS') != '1':
 raise SystemExit('Set ARCHY_ALLOW_DISPOSABLE_CONTAINERS=1 to run isolated test containers')
manifest=yaml.safe_load((pathlib.Path(__file__).resolve().parents[2]/'apps/angor-indexer/manifest.yml').read_text())['app']
health=manifest['health_check']
health_url=health['endpoint'].rstrip('/')+health.get('path','/')
run_id=uuid.uuid4().hex[:12]
net='archy-angor-test-'+run_id;backend='angor-test-backend-'+run_id;gateway='angor-test-gateway-'+run_id;frontend='angor-test-frontend-'+run_id
port=None
def run(*a):
 r=subprocess.run(a,capture_output=True,text=True)
 if r.returncode:raise RuntimeError(r.stderr)
 return r.stdout.strip()
def req(path,data=None,method=None,headers={}):
 r=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=data,method=method,headers=headers)
 try:
  with urllib.request.urlopen(r,timeout=10) as f:return f.status,f.headers,f.read()
 except urllib.error.HTTPError as e:return e.code,e.headers,e.read()
script="""const server=require('http').createServer((q,r)=>{let b='';q.on('data',x=>b+=x);q.on('end',()=>{r.setHeader('Access-Control-Allow-Origin','https://wrong.example');if(q.url==='/api/v1/blocks/tip/height'){r.end('900000');return}r.setHeader('Content-Type','application/json');r.end(JSON.stringify({url:q.url,method:q.method,body:b,cookie:q.headers.cookie||null,auth:q.headers.authorization||null}))})});server.on('upgrade',(q,s)=>{if(q.url!=='/api/v1/ws'||q.headers.cookie||q.headers.authorization){s.destroy();return}const accept=require('crypto').createHash('sha1').update(q.headers['sec-websocket-key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');s.end('HTTP/1.1 101 Switching Protocols\\r\\nUpgrade: websocket\\r\\nConnection: Upgrade\\r\\nSec-WebSocket-Accept: '+accept+'\\r\\n\\r\\n'+String.fromCharCode(129,11)+'fixture-tip','latin1')});server.listen(8999,'0.0.0.0')"""
frontend_script="""require('http').createServer((q,r)=>{if(q.headers.cookie||q.headers.authorization){r.writeHead(500);r.end('credential leak');return}if(q.url==='/asset.js'){r.setHeader('Content-Type','application/javascript');r.end('window.explorer=true');return}if(q.url==='/'||q.url==='/tx/fixture'){r.setHeader('Content-Type','text/html');r.end('<html>Mempool explorer fixture</html>');return}r.writeHead(404);r.end('not found')}).listen(8080,'0.0.0.0')"""
def start_frontend():run('podman','run','-d','--name',frontend,'--network',net,'--network-alias','mempool','--cap-drop=all','--security-opt=no-new-privileges','docker.io/library/node:24-alpine','node','-e',frontend_script)
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
 start_backend();start_frontend()
 run('podman','run','-d','--name',gateway,'--network',net,'--read-only','--cap-drop=all','--security-opt=no-new-privileges','--memory','128m','--health-cmd','wget -q -T 5 -O /dev/null '+shlex.quote(health_url),'--health-interval','5s','--health-retries','2','-p','127.0.0.1::8080',manifest['container']['image'])
 port=int(json.loads(run('podman','inspect',gateway))[0]['NetworkSettings']['Ports']['8080/tcp'][0]['HostPort'])
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
 for path in ['/', '/tx/fixture', '/asset.js']:
  status,headers,body=req(path,headers={'Cookie':'private=secret','Authorization':'Bearer secret'})
  assert status==200 and (b'explorer' in body), (path,status,body)
 assert req('/',b'not-allowed')[0]==403
 with socket.create_connection(('127.0.0.1',port),timeout=10) as stream:
  stream.sendall(b'GET /api/v1/ws HTTP/1.1\r\nHost: indexer.example\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nCookie: private=secret\r\nAuthorization: Bearer secret\r\n\r\n')
  response=b''
  while b'fixture-tip' not in response:
   chunk=stream.recv(4096)
   if not chunk:break
   response+=chunk
  assert b'101 Switching Protocols' in response and b'fixture-tip' in response,response
 print('PASS shared explorer root/assets/deep links and real WebSocket upgrade/frame; credentials stripped',flush=True)
 run('podman','stop',frontend)
 assert req('/')[0]==503
 assert req('/health')[0]==200
 run('podman','rm',frontend);start_frontend()
 end=time.monotonic()+40
 while time.monotonic()<end:
  if req('/')[0]==200:break
  time.sleep(1)
 else:raise RuntimeError('Frontend recreation did not recover')
 print('PASS frontend outage is explicit; API stays available; frontend DNS recreation recovers',flush=True)
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
 for name in [gateway,backend,frontend]:subprocess.run(['podman','rm','-f','--time','3',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 subprocess.run(['podman','network','rm',net],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
