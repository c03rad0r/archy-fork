#!/usr/bin/env python3
"""Opt-in built-demo acceptance; disposable containers, no production data mounts."""
import pathlib,subprocess,tempfile,uuid,json,time,urllib.request,os
if os.environ.get('ARCHY_ALLOW_DISPOSABLE_CONTAINERS') != '1':
 raise SystemExit('Set ARCHY_ALLOW_DISPOSABLE_CONTAINERS=1 for isolated demo tests')
root=pathlib.Path(__file__).resolve().parents[2]
web_image=os.environ['ARCHY_DEMO_WEB_IMAGE']
backend_image=os.environ['ARCHY_DEMO_BACKEND_IMAGE']
source_revision=os.environ['ARCHY_DEMO_SOURCE_REVISION']
assert len(source_revision)==40 and all(c in '0123456789abcdef' for c in source_revision)
uid=uuid.uuid4().hex[:10];net='archy-release-demo-'+uid;backend=net+'-backend';web=net+'-web'
def run(*args):return subprocess.check_output(args,text=True,stderr=subprocess.STDOUT,timeout=60).strip()
dns_fixture=tempfile.TemporaryDirectory(prefix='archy-demo-dns-')
resolv=pathlib.Path(dns_fixture.name)/'resolv.conf'
resolv.write_text('nameserver 127.0.0.1\n')
try:
 run('podman','network','create',net)
 run('podman','run','-d','--name',backend,'--network',net,'--network-alias','neode-backend','--memory','1g','-p','127.0.0.1::5959','-e','DEMO=1','-e','DEMO_FILE_QUOTA_BYTES=8388608',backend_image)
 backend_config=json.loads(run('podman','inspect','--type','container',backend))[0]
 backend_ip=next(iter(backend_config['NetworkSettings']['Networks'].values()))['IPAddress']
 run('podman','run','-d','--name',web,'--dns','none','-v',str(resolv)+':/etc/resolv.conf:ro','--add-host','neode-backend:'+backend_ip,'-p','127.0.0.1::2101','--network',net,'--memory','256m','-p','127.0.0.1::80',web_image)
 front='http://'+run('podman','port',web,'80/tcp');back='http://'+run('podman','port',backend,'5959/tcp')
 end=time.monotonic()+120
 while time.monotonic()<end:
  try:
   with urllib.request.urlopen(back+'/health',timeout=3) as r:assert r.status==200
   with urllib.request.urlopen(front+'/',timeout=3) as r:assert r.status==200
   break
  except Exception:
   if run('podman','inspect','--type','container','--format','{{.State.Status}}',web)=='exited':
    print(run('podman','logs','--tail','12',web),flush=True)
    raise RuntimeError('Demo web exited before readiness')
   time.sleep(2)
 else:raise RuntimeError('Demo image health timeout')
 print('PASS actual demo web/backend images ready on private loopback ports',flush=True)
 assert 'nameserver 127.0.0.1' in run('podman','exec',web,'cat','/etc/resolv.conf')
 optional='http://'+run('podman','port',web,'2101/tcp')
 try:
  urllib.request.urlopen(optional,timeout=10)
  raise AssertionError('Expected unavailable optional upstream with DNS disabled')
 except urllib.error.HTTPError as error:assert error.code==502
 with urllib.request.urlopen(front+'/',timeout=5) as response:assert response.status==200
 print('PASS unavailable optional DNS returns502 while the demo stays200',flush=True)
 with urllib.request.urlopen(front+'/aiui/BUILD-INFO',timeout=5) as r:assert r.read().decode().startswith('commit='+source_revision+'\n')
 config=json.loads(run('podman','inspect','--type','container',backend))[0]
 assert config['Config']['Healthcheck']['Test']
 assert not config.get('Mounts'),'Demo must have no production socket/data mounts'
 print('PASS current AIUI provenance, backend healthcheck and no host-data mounts',flush=True)
 original=(root/'neode-ui/scripts/demo-upload-test.mjs').read_text()
 original=original.replace("const root = new URL('../', import.meta.url)","const root = new URL("+json.dumps((root/'neode-ui').as_uri()+'/')+")")
 start=original.index('before(async () => {');end=original.index('after(async () => {',start)
 original=original[:start]+"before(async () => { origin = "+json.dumps(front)+" })\n"+original[end:]
 original=original.replace('const response = await fetch(`${origin}/health`)', 'const response = await fetch('+json.dumps(back+'/health')+')')
 # Keep this file beside the source so Node resolves the project's TypeScript.
 test=root/'neode-ui/scripts'/('.demo-image-acceptance-'+uid+'.mjs')
 test.write_text(original)
 try:subprocess.run(['node','--test',str(test)],cwd=root,check=True,timeout=180)
 finally:test.unlink(missing_ok=True)
 print('PASS all production upload protocol cases through the built nginx image',flush=True)
 (pathlib.Path('/tmp/archy-190-demo-image-ports.json')).write_text(json.dumps({'front':front,'back':back,'web':web,'backend':backend}))
 # Browser smoke, using the real public-demo login page.
 script=r'''
const {createRequire}=require('node:module');const req=createRequire(process.cwd()+'/neode-ui/package.json');const {chromium}=req('@playwright/test');
(async()=>{const browser=await chromium.launch({headless:true});try {const page=await browser.newPage({viewport:{width:390,height:844}});await page.goto(process.env.DEMO_IMAGE_URL,{waitUntil:'domcontentloaded'});await page.locator('.tap-to-start-content').click({timeout:30000});await page.getByRole('button',{name:'Skip Intro',exact:true}).click();await page.waitForURL('**/onboarding/intro',{timeout:45000});await page.screenshot({path:'/tmp/archy-190-demo-entry.png',fullPage:true});console.log('Demo entry route:',new URL(page.url()).pathname);await page.getByRole('button',{name:'Enter the demo →',exact:true}).click();const password=page.locator('input[type=password]').first();await password.waitFor({timeout:30000});await password.fill('entertoexit');await password.press('Enter');await page.getByText('My Apps',{exact:true}).first().waitFor({timeout:45000});if(await password.isVisible())throw Error('Demo login did not reach dashboard');await page.screenshot({path:'/tmp/archy-190-demo-mobile.png',fullPage:true});console.log('PASS built demo mobile login and dashboard');}finally{await browser.close()}})().catch(e=>{console.error(e);process.exit(1)});
'''
 subprocess.run(['node','-e',script],cwd=root,env=os.environ|{'DEMO_IMAGE_URL':front},check=True,timeout=120)
finally:
 for name in [web,backend]:subprocess.run(['podman','rm','-f',name],capture_output=True)
 subprocess.run(['podman','network','rm',net],capture_output=True)
 dns_fixture.cleanup()
 print('Disposable demo containers and network removed',flush=True)
