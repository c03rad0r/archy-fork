#!/usr/bin/env python3
"""Operation-owned legacy IndeeHub maintenance. Called only by supervised updater.
No live execution is part of source qualification. Original writable-layer images
must already be durable. Never unlock ARCHY_UPDATE_LOCK_FD or release another hold.
"""
import datetime, hashlib, json, os, pathlib, re, shutil, subprocess, sys, time, uuid, tarfile
NAMES = ('indeedhub','indeedhub-api','indeedhub-ffmpeg','indeedhub-minio','indeedhub-postgres','indeedhub-redis','indeedhub-relay')
VOLUMES = ('indeedhub-minio-data','indeedhub-postgres-data','indeedhub-redis-data','indeedhub-relay-data')
DATA = pathlib.Path('/var/lib/archipelago')
QUEUE_COUNTS = frozenset(('active','waiting','paused','delayed','failed','completed'))
def valid_queue_counts(counts):
    return isinstance(counts,dict) and set(counts)==QUEUE_COUNTS and all(type(v) is int and v>=0 for v in counts.values())
BUSINESS_COUNTS = frozenset(('projects','contents','payments','shareholders','subscriptions','library_items','other_active_transactions'))
def valid_empty_business(counts):
    return isinstance(counts,dict) and set(counts)==BUSINESS_COUNTS and all(type(v) is int and v==0 for v in counts.values())
def valid_empty_queue(observed):
    return isinstance(observed,dict) and set(observed)=={'paused','counts','extra'} and observed['paused'] is True and valid_queue_counts(observed['counts']) and all(v==0 for v in observed['counts'].values()) and isinstance(observed['extra'],dict) and set(observed['extra'])=={'prioritized','waiting_children'} and all(type(v) is int and v==0 for v in observed['extra'].values())
def valid_api_process_proof(proof):
    if not isinstance(proof,dict) or set(proof)!={'parent','child'}:return False
    for part in proof.values():
        if not isinstance(part,dict) or set(part)!={'pid','ppid','starttime','command'}:return False
        if type(part['pid']) is not int or type(part['ppid']) is not int or not isinstance(part['starttime'],str) or not re.fullmatch('[0-9]+',part['starttime']) or not isinstance(part['command'],str):return False
    parent=proof['parent'];child=proof['child']
    return parent['pid']==1 and parent['ppid']==0 and parent['command'].rstrip('\0')=='npm run start:prod' and child['pid']>1 and child['ppid']==1 and child['command']=='node\0dist/main\0'
QUEUE_SCRIPT = r'''const {Queue}=require('bullmq');
(async()=>{const q=new Queue('transcode',{connection:{host:process.env.QUEUE_HOST,port:Number(process.env.QUEUE_PORT||6379),password:process.env.QUEUE_PASSWORD,maxRetriesPerRequest:1}});
try{const action=process.argv[1];if(action==='pause')await q.pause();else if(action==='resume')await q.resume();else if(action!=='status')throw Error('action');
console.log(JSON.stringify({paused:await q.isPaused(),prefix:q.toKey(''),counts:await q.getJobCounts('active','waiting','paused','delayed','failed','completed')}));}
finally{await q.close()}})().catch(()=>process.exit(1));'''

REDIS_OBSERVE_SCRIPT = "-- Atomic observation only. KEYS[1] must be the previously captured and validated\n-- exact queue prefix including trailing colon (for example bull:transcode:).\n-- No Queue constructor, marker deletion, pause/resume or data write occurs.\nlocal p = KEYS[1]\nif #KEYS ~= 1 or #ARGV ~= 0 or not p or #p == 0 then\n  return redis.error_reply('Invalid bound queue prefix')\nend\nlocal counts = {\n  active = redis.call('LLEN', p .. 'active'),\n  waiting = redis.call('LLEN', p .. 'wait'),\n  paused = redis.call('LLEN', p .. 'paused'),\n  delayed = redis.call('ZCARD', p .. 'delayed'),\n  failed = redis.call('ZCARD', p .. 'failed'),\n  completed = redis.call('ZCARD', p .. 'completed')\n}\nreturn cjson.encode({\n  paused = redis.call('HEXISTS', p .. 'meta', 'paused') == 1,\n  counts = counts,\n  extra = {\n    prioritized = redis.call('ZCARD', p .. 'prioritized'),\n    waiting_children = redis.call('ZCARD', p .. 'waiting-children')\n  }\n})\n"
API_PROCESS_SCRIPT = r'''const fs=require('fs');
const command=p=>fs.readFileSync(`/proc/${p}/cmdline`,'utf8');
const parent=p=>Number(fs.readFileSync(`/proc/${p}/status`,'utf8').match(/^PPid:\s+(\d+)$/m)?.[1]);
const identity=p=>{const value=fs.readFileSync(`/proc/${p}/stat`,'utf8');return {pid:Number(p),ppid:parent(p),starttime:value.slice(value.lastIndexOf(') ')+2).split(' ')[19],command:command(p)}};
function observe(){
 if(command(1).replace(/\0+$/,'')!=='npm run start:prod')throw Error('Unrecognized API parent');
 const children=fs.readdirSync('/proc').filter(p=>/^\d+$/.test(p)).filter(p=>{try{return parent(p)===1}catch{return false}});
 if(children.length!==1)throw Error('Ambiguous API children');
 const child=identity(children[0]);if(child.command!=='node\0dist/main\0'||child.ppid!==1||!/^\d+$/.test(child.starttime))throw Error('Unrecognized API child');
 return {parent:identity(1),child};
}
const proof=observe(),action=process.argv[1];
if(action==='signal'){
 const expected=JSON.parse(process.argv[2]);if(JSON.stringify(proof)!==JSON.stringify(expected)||JSON.stringify(observe())!==JSON.stringify(expected))throw Error('API process identity changed');
 process.kill(proof.child.pid,'SIGTERM');fs.writeSync(1,JSON.stringify({proof,signal:'SIGTERM',acknowledged:true})+'\n');
}else if(action==='probe')fs.writeSync(1,JSON.stringify(proof)+'\n');else throw Error('Unsupported action');'''
LEGACY_API_CMD = ['sh','-c',"echo 'Running database migrations...' && npx typeorm migration:run -d dist/database/ormconfig.js && echo 'Migrations complete.' && npm run start:prod"]

# The exact three migrations in the privately qualified API candidate. This is
# an allowlist of additive schema history, never permission to discard app data.
ADDITIVE_MIGRATIONS = {
    'AddArchipelagoPublicationsAndRentals1791288000000':1791288000000,
    'AddMediaRegistrationIntents1791374400000':1791374400000,
    'AddMediaRegistrationRetirements1791374401000':1791374401000,
}
ADDITIVE_TABLES = {'archipelago_media_registrations','archipelago_publications','archipelago_publication_outbox','archipelago_rental_entitlements','archipelago_registration_intents'}
DB_COMMITMENTS_SQL = r'''
BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT format($query$
 SELECT jsonb_build_object('table',%L,'schema',
   jsonb_build_object(
    'columns',(SELECT coalesce(jsonb_agg(jsonb_build_array(a.attnum,a.attname,format_type(a.atttypid,a.atttypmod),a.attnotnull,a.attidentity,a.attgenerated,pg_get_expr(d.adbin,d.adrelid)) ORDER BY a.attnum),'[]'::jsonb) FROM pg_attribute a LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE a.attrelid=%s AND a.attnum>0 AND NOT a.attisdropped),
    'constraints',(SELECT coalesce(jsonb_agg(jsonb_build_array(conname,pg_get_constraintdef(oid,true)) ORDER BY conname),'[]'::jsonb) FROM pg_constraint WHERE conrelid=%s),
    'indexes',(SELECT coalesce(jsonb_agg(pg_get_indexdef(indexrelid) ORDER BY indexrelid::regclass::text),'[]'::jsonb) FROM pg_index WHERE indrelid=%s),
    'triggers',(SELECT coalesce(jsonb_agg(pg_get_triggerdef(oid,true) ORDER BY tgname),'[]'::jsonb) FROM pg_trigger WHERE tgrelid=%s AND NOT tgisinternal),
    'rls',%L,'policies',(SELECT coalesce(jsonb_agg(to_jsonb(p) ORDER BY policyname),'[]'::jsonb) FROM pg_policies p WHERE schemaname='public' AND tablename=%L)),
   'rows',count(*),'rows_sha256',encode(sha256(convert_to(coalesce(string_agg(to_jsonb(t)::text,E'\n' ORDER BY to_jsonb(t)::text),''),'UTF8')),'hex')) FROM public.%I t;
 $query$,c.relname,c.oid,c.oid,c.oid,c.oid,c.relrowsecurity::text||':'||c.relforcerowsecurity::text,c.relname,c.relname)
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p') ORDER BY c.relname
\gexec
SELECT jsonb_build_object('migration_rows',coalesce(jsonb_agg(to_jsonb(m) ORDER BY id),'[]'::jsonb)) FROM public.migrations m;
COMMIT;
'''
def verify_database_compatibility(before, after):
    require(before.get('operation_id')==after.get('operation_id'),'Data compatibility operation changed')
    old=before['tables'];new=after['tables'];require(set(old)<=set(new),'Data compatibility lost original tables')
    extra=set(new)-set(old);require(extra<=ADDITIVE_TABLES,'Data compatibility contains unreviewed tables')
    for name in old:
        require(old[name]['schema']==new[name]['schema'],'Data compatibility changed original table schema')
        if name!='migrations':
            require(old[name]['rows']==new[name]['rows'] and old[name]['rows_sha256']==new[name]['rows_sha256'],'Data compatibility changed original rows')
    for name in extra:require(new[name]['rows']==0,'Data compatibility contains new application data')
    previous=before['migrations'];current=after['migrations']
    require(current[:len(previous)]==previous,'Data compatibility changed original migration history')
    added=current[len(previous):];seen=set()
    for row in added:
        require(set(row)=={'id','timestamp','name'} and type(row['id']) is int and row['name'] not in seen and ADDITIVE_MIGRATIONS.get(row['name'])==row['timestamp'],'Data compatibility has unreviewed migration history')
        seen.add(row['name'])
    ordered=list(ADDITIVE_MIGRATIONS)
    require([row['name'] for row in added]==ordered[:len(added)],'Data compatibility migration order changed')
    expected_extra=(ADDITIVE_TABLES-{'archipelago_registration_intents'}) if added else set()
    if len(added)>=2:expected_extra=ADDITIVE_TABLES
    require(extra==expected_extra-set(old),'Data compatibility additions do not match reviewed migration evidence')
    return {'original_tables':len(old),'new_empty_tables':sorted(extra),'reviewed_migrations':[r['name'] for r in added]}

def require(condition, message):
    if not condition: raise RuntimeError(message)
def atomic(path, value):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    require(not path.is_symlink(), 'Refuse symbolic journal path')
    temporary=path.with_name('.'+path.name+'.'+str(uuid.uuid4()))
    with temporary.open('x') as stream:
        json.dump(value,stream,separators=(',',':'));stream.flush();os.fsync(stream.fileno())
    os.chmod(temporary,0o600);os.replace(temporary,path)
    descriptor=os.open(path.parent,os.O_RDONLY);os.fsync(descriptor);os.close(descriptor)
def sha(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def validate_members(members):
    require(isinstance(members,list) and len(members)==7,'Seven exact original members required')
    require({m.get('name') for m in members}==set(NAMES),'IndeeHub member scope changed')
    for m in members:
        require(set(m)=={'name','container_id','image_id','unit_sha256','config_sha256','running'},'Unexpected member fields')
        for key in ('container_id','image_id','unit_sha256','config_sha256'):
            require(bool(re.fullmatch('[0-9a-f]{64}',m[key])),'Invalid original identity/hash')
        require(m['running'] is True,'Legacy barrier currently supports an originally running complete stack only')
    return sorted(members,key=lambda m:m['name'])
def validate_nginx_guards(config):
    guard='if (-f /var/lib/archipelago/app-maintenance/indeedhub) { return 503; }'
    lines=config.splitlines();matched=0;index=0
    while index<len(lines):
        header=lines[index]
        if not re.match(r'^\s*location\b.*\{\s*$',header):index+=1;continue
        block=[];depth=0
        while index<len(lines):
            line=lines[index];block.append(line)
            unquoted=re.sub(r"(['\"])(?:\\.|(?!\1).)*\1",'',line).split('#',1)[0]
            depth+=unquoted.count('{')-unquoted.count('}');index+=1
            if depth==0:break
        body='\n'.join(block)
        if '/app/indeedhub' in header or re.search(r'proxy_pass\s+https?://127[.]0[.]0[.]1:7778(?:/|;)',body):
            require(bool(re.match(r'^\s*location\s+(?:\^~\s+|=\s+)?/app/indeedhub(?:/[^\s{]*)?\s*\{\s*$',header)),'Unrecognized direct IndeeHub proxy exposure')
            require(guard in body,'An IndeeHub proxy route is missing its maintenance guard')
            matched+=1
    require(matched>=3,'Expected complete legacy IndeeHub route guards')
    return matched
def validate_volume_archive(path):
    """Validate the complete inventory before extracting into private storage."""
    entries={};size=0
    with tarfile.open(path, 'r:') as archive:
        for member in archive:
            name=pathlib.PurePosixPath(member.name)
            require(not name.is_absolute() and '..' not in name.parts,'Unsafe volume archive path')
            key=str(name)
            require(key not in entries and len(entries)<1000000,'Duplicate or oversized volume archive inventory')
            require(member.isfile() or member.isdir() or member.issym() or member.islnk(),'Unsupported volume archive entry')
            entries[key]=member;size+=member.size
    require(entries and entries.get('.') and entries['.'].isdir(),'Volume archive root is missing')
    for key,member in entries.items():
        for parent in pathlib.PurePosixPath(key).parents:
            ancestor=entries.get(str(parent))
            require(ancestor is None or ancestor.isdir(),'Volume archive writes through a link')
        if member.issym() or member.islnk():
            target=pathlib.PurePosixPath(member.linkname)
            require(not target.is_absolute(),'Volume archive link escapes restore storage')
            parts=list(pathlib.PurePosixPath(key).parent.parts) if member.issym() else []
            for part in target.parts:
                if part=='..':
                    require(parts,'Volume archive link escapes restore storage');parts.pop()
                elif part!='.':parts.append(part)
            if member.islnk():
                linked=entries.get(str(pathlib.PurePosixPath(*parts)))
                require(linked is not None and linked.isfile(),'Volume archive hardlink target is not a regular file')
    return size

def volume_archive_metadata(path):
    entries={};links={}
    with tarfile.open(path,'r:') as archive:
        for member in archive:
            name=str(pathlib.PurePosixPath(member.name))
            entries[name]={'mode':member.mode,'uid':member.uid,'gid':member.gid,
                'attributes':{key:value for key,value in member.pax_headers.items() if key.startswith('SCHILY.')}}
            if member.islnk():links[name]=str(pathlib.PurePosixPath(member.linkname))
    for name,target in links.items():entries[name]=entries[target]
    return entries

class Controller:
    def __init__(self, data, operation, lock_fd, runner=None):
        self.data=pathlib.Path(data);self.operation=operation;self.lock_fd=lock_fd;self.runner=runner
        self.runtime_root=pathlib.Path('/run/user')/str(os.getuid())
        require(str(uuid.UUID(operation))==operation,'Invalid operation UUID')
        self.root=self.data/'update-transactions'/'indeehub-maintenance'/operation
        self.path=self.root/'journal.json';self.fence=self.data/'app-maintenance'/'indeedhub'
        self.record=json.loads(self.path.read_text()) if self.path.exists() else None
        if self.record:require(self.record['operation_id']==operation,'Maintenance journal changed')
    def save(self): atomic(self.path,self.record)
    def run(self, argv, timeout=30, output=None, input_bytes=None, input_file=None):
        if self.runner:return self.runner(argv,timeout,output)
        self.root.mkdir(mode=0o700,parents=True,exist_ok=True)
        with (self.root/'commands.private.log').open('ab') as errors:
            result=subprocess.run(argv,stdout=output or subprocess.PIPE,stderr=errors,timeout=timeout,check=True,pass_fds=(self.lock_fd,),input=input_bytes,stdin=input_file)
        if output:return b''
        require(len(result.stdout)<=2*1024*1024,'Command response exceeds bound')
        return result.stdout
    def inspect(self, name):
        rows=json.loads(self.run(['podman','inspect',name]));require(len(rows)==1,'Unexpected container inspection');return rows[0]
    def holds(self):
        for name in NAMES:
            path=self.data/'update-transactions'/'holds'/name
            require(path.is_file() and not path.is_symlink() and path.read_text()==self.operation,'Matching durable lifecycle hold required')
    def fence_matches(self):
        require(self.fence.is_file() and not self.fence.is_symlink() and self.fence.read_text()==self.operation,'Admission fence ownership changed')
    def close_ingress(self):
        # The deployed native AppGate and legacy nginx guards consume this exact
        # sentinel. This code never edits arbitrary nginx configuration.
        # nginx -T tests its pid file as well as reading configuration. The
        # manager's strict mount namespace makes /run/nginx.pid read-only, even
        # after sudo. Use one fixed read-only command in PID1's fresh service
        # context; do not broaden the manager's writable paths or detach the
        # controller that owns the inherited lifecycle lock.
        config=self.run(['sudo','-n','/usr/bin/systemd-run','--quiet','--wait',
                         '--pipe','--collect','--','/usr/sbin/nginx','-T']).decode()
        validate_nginx_guards(config)
        self.fence.parent.mkdir(mode=0o755,exist_ok=True)
        self.fence.parent.chmod(0o755)
        require(not self.fence.parent.is_symlink(),'Admission directory is a symlink')
        if self.fence.exists():self.fence_matches()
        else:
            with self.fence.open('x') as stream:stream.write(self.operation);stream.flush();os.fsync(stream.fileno())
            self.fence.chmod(0o644)
            fd=os.open(self.fence.parent,os.O_RDONLY);os.fsync(fd);os.close(fd)
        # A local legacy probe must be rejected without entering the old app.
        import urllib.request,urllib.error
        try:
            urllib.request.urlopen('http://127.0.0.1/app/indeedhub/__maintenance_probe',timeout=5)
            raise RuntimeError('Legacy ingress was not fenced')
        except urllib.error.HTTPError as error:
            require(error.code==503,'Legacy ingress guard did not return maintenance status')
        self.record['ingress_closed']=True;self.save()
    def queue(self, action):
        result=json.loads(self.run(['podman','exec','indeedhub-api','node','-e',QUEUE_SCRIPT,action]))
        require(type(result.get('paused')) is bool and valid_queue_counts(result.get('counts')),'Incomplete or invalid queue observation')
        return result
    def pause_queue(self):
        if 'queue_was_paused' not in self.record:
            original=self.queue('status');self.record['queue_was_paused']=original['paused'];self.record['queue_original_counts']=original['counts'];self.save()
        state=self.queue('pause');require(state['paused'],'Worker admission did not close')
        self.record['queue_pause_confirmed']=True;self.save()
    def legacy_api_idle(self):
        # Narrow first-upgrade compatibility for the observed legacy API which
        # has no SIGTERM hooks. Existing customer/business work is never inferred
        # completed: this path requires a fresh empty store behind closed ingress.
        require(self.record.get('stopped',{}).get('indeedhub',{}).get('confirmed'),'Frontend ingress must already be stopped')
        require(self.record.get('stopped',{}).get('indeedhub-ffmpeg',{}).get('confirmed'),'Transcode worker must already be stopped')
        require(self.record.get('queue_pause_confirmed') is True and valid_queue_counts(self.record.get('last_queue_counts')) and self.record['last_queue_counts']['active']==0,'Worker queue is not proven idle')
        tables=('projects','contents','payments','shareholders','subscriptions','library_items')
        fields=','.join("'%s',(SELECT count(*) FROM public.%s)"%(name,name) for name in tables)
        sql="SELECT json_build_object("+fields+",'other_active_transactions',(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND state<>'idle'))"
        counts=json.loads(self.run(['podman','exec','indeedhub-postgres','psql','-XAt','-U','indeedhub','-d','indeedhub','-c',sql]))
        require(set(counts)==set(tables)|{'other_active_transactions'},'Legacy API business-state observation incomplete')
        require(all(type(value) is int and value==0 for value in counts.values()),'Legacy API has business work or active transactions; completion cannot be inferred')
        self.record['legacy_api_empty_state']=counts;self.save()
    def api_recovery_identity(self, member):
        self.holds();self.fence_matches()
        require(member['name']=='indeedhub-api','Legacy API member required')
        runtime=json.loads((self.data/'update-transactions'/'supervised'/(self.operation+'.json')).read_text())
        require(runtime.get('id')==self.operation and runtime.get('phase') in ('Editing','Restoring') and runtime.get('target_startup_began') is False,'Legacy API operation changed')
        rows=[m for m in runtime['members'] if m['original']['name']==member['name']]
        require(len(rows)==1,'Ambiguous legacy API recovery identity');original=rows[0]['original'];recovery=rows[0]['recovery_image']
        require(original['container_id']==member['container_id'] and original['image'].removeprefix('sha256:')==member['image_id'].removeprefix('sha256:') and original['config_sha256']==member['config_sha256'] and hashlib.sha256(original['body'].encode()).hexdigest()==member['unit_sha256'],'Legacy API original identity changed')
        require(recovery['source_container_id']==member['container_id'] and recovery['operation_id']==self.operation,'Legacy API recovery identity changed')
        images=json.loads(self.run(['podman','image','inspect',recovery['image']]))
        require(len(images)==1 and images[0]['Id'].removeprefix('sha256:')==recovery['image'].removeprefix('sha256:'),'Legacy API recovery image changed')
        image=images[0];require(image['Config'].get('Cmd')==LEGACY_API_CMD and image['Config'].get('Entrypoint')==['docker-entrypoint.sh'],'Unrecognized legacy API command')
        source=pathlib.Path(self.run(['systemctl','--user','show',member['name']+'.service','--property=SourcePath','--value']).decode().strip())
        require(source.is_file() and not source.is_symlink() and source.suffix=='.container' and source.stat().st_uid==os.getuid() and sha(source)==member['unit_sha256'],'Legacy API saved unit changed')
        return image
    def api_queue_binding(self, member, api, image):
        def environment(config):
            result={}
            for value in config.get('Env',[]):
                key,_,text=value.partition('=')
                if key in ('QUEUE_HOST','QUEUE_PORT','QUEUE_PASSWORD'):
                    require(key not in result,'Ambiguous queue endpoint');result[key]=text
            return result
        expected=environment(image['Config']);actual=environment(api['Config'])
        require(actual==expected and expected.get('QUEUE_PORT','6379')=='6379','Original API queue endpoint changed')
        redis=next(m for m in self.record['original_members'] if m['name']=='indeedhub-redis')
        store=self.inspect(redis['name']);require(store['Id']==redis['container_id'] and store['Image']==redis['image_id'],'Original queue store changed')
        api_networks=api['NetworkSettings']['Networks'];networks=store['NetworkSettings']['Networks']
        host=expected.get('QUEUE_HOST');matches=[]
        for name,network in networks.items():
            if name in api_networks and api_networks[name]['NetworkID']==network['NetworkID'] and host in (network.get('Aliases') or []):matches.append(network['NetworkID'])
        require(len(matches)==1,'API queue endpoint is not bound to original Redis network alias')
        return {'container_id':redis['container_id'],'image_id':redis['image_id'],'network_id':matches[0],'host':host,'port':6379}
    def require_api_stopped(self, member):
        require(not self.run(['podman','ps','--no-trunc','--filter','name=^'+member['name']+'$','--format','{{.ID}}']).strip(),'An API writer is still running')
        intent=self.record['stopped'][member['name']]['intent_at']
        events=self.run(['podman','events','--stream=false','--since',str(int(intent)-1),'--filter','container='+member['container_id'],'--filter','event=oom','--format','json'])
        require(not events.strip(),'API OOM evidence prevents compatibility termination')
    def observe_empty_redis_queue(self, member, prefix, binding=None):
        require(prefix=='bull:transcode:','Unrecognized bound queue prefix')
        redis=next(m for m in self.record['original_members'] if m['name']=='indeedhub-redis')
        actual=self.inspect(redis['name']);require(actual['Id']==redis['container_id'] and actual['Image']==redis['image_id'],'Original queue store changed')
        image=self.api_recovery_identity(member)
        if binding is not None:
            networks=actual['NetworkSettings']['Networks']
            require(binding.get('container_id')==redis['container_id'] and binding.get('image_id')==redis['image_id'] and binding.get('port')==6379 and sum(network.get('NetworkID')==binding.get('network_id') and binding.get('host') in (network.get('Aliases') or []) for network in networks.values())==1,'Original queue endpoint binding changed')
        values=[v.split('=',1)[1] for v in image['Config'].get('Env',[]) if v.startswith('QUEUE_PASSWORD=')]
        require(len(values)<=1,'Ambiguous queue credential');password=values[0] if values else ''
        require('\n' not in password and '\r' not in password,'Unsupported queue credential framing')
        shell='IFS= read -r secret || exit 1; if [ -n "$secret" ]; then REDISCLI_AUTH="$secret"; export REDISCLI_AUTH; else unset REDISCLI_AUTH; fi; exec redis-cli --no-auth-warning --raw EVAL "$1" 1 "$2"'
        observed=json.loads(self.run(['podman','exec','-i',redis['container_id'],'sh','-c',shell,'queue-observer',REDIS_OBSERVE_SCRIPT,prefix],input_bytes=(password+'\n').encode()))
        require(observed.get('paused') is True and valid_queue_counts(observed.get('counts')) and all(v==0 for v in observed['counts'].values()),'Legacy API queue not paused and empty')
        extra=observed.get('extra');require(isinstance(extra,dict) and set(extra)=={'prioritized','waiting_children'} and all(type(v) is int and v==0 for v in extra.values()),'Legacy API has additional queued work')
        return observed
    def api_restart_override_path(self):
        require(self.runtime_root.is_dir() and not self.runtime_root.is_symlink() and self.runtime_root.stat().st_uid==os.getuid() and self.runtime_root.stat().st_mode & 0o022==0,'Unsafe user runtime directory')
        parent=self.runtime_root
        for name in ('systemd','user','indeedhub-api.service.d'):
            parent=parent/name
            parent.mkdir(mode=0o700,exist_ok=True)
            require(parent.is_dir() and not parent.is_symlink() and parent.stat().st_uid==os.getuid() and parent.stat().st_mode & 0o022==0,'Unsafe API restart override directory')
        return parent/('zz-archipelago-maintenance-'+self.operation+'.conf')
    def api_restart_override_bytes(self):
        return ('# Archipelago maintenance operation '+self.operation+'\n[Service]\nRestart=no\n').encode()
    def verify_api_restart_override(self, path):
        require(path.is_file() and not path.is_symlink() and path.stat().st_uid==os.getuid() and path.stat().st_mode & 0o777==0o600 and path.read_bytes()==self.api_restart_override_bytes(),'API restart override changed; hold retained')
    def ensure_api_restart_override(self):
        self.holds();self.fence_matches();path=self.api_restart_override_path();saved=self.record.get('api_restart_override')
        if saved is None:
            require(not path.exists() and not path.is_symlink(),'Unowned API restart override exists')
            policy=self.run(['systemctl','--user','show','indeedhub-api.service','--property=Restart','--value']).decode().strip()
            require(policy in ('no','always','on-success','on-failure','on-abnormal','on-watchdog','on-abort'),'Unrecognized original restart policy')
            saved={'operation_id':self.operation,'original_policy':policy,'sha256':hashlib.sha256(self.api_restart_override_bytes()).hexdigest(),'released':False}
            self.record['api_restart_override']=saved;self.save()
        require(saved.get('operation_id')==self.operation and saved.get('sha256')==hashlib.sha256(self.api_restart_override_bytes()).hexdigest() and saved.get('released') is False,'API restart override obligation changed')
        if path.exists() or path.is_symlink():self.verify_api_restart_override(path)
        else:
            descriptor=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
            with os.fdopen(descriptor,'wb') as stream:stream.write(self.api_restart_override_bytes());stream.flush();os.fsync(stream.fileno())
            directory=os.open(path.parent,os.O_RDONLY);os.fsync(directory);os.close(directory)
        self.run(['systemctl','--user','daemon-reload'])
        require(self.run(['systemctl','--user','show','indeedhub-api.service','--property=Restart','--value']).decode().strip()=='no','API automatic restart did not close')
        saved['installed']=True;self.save()
    def release_api_restart_override(self):
        saved=self.record.get('api_restart_override')
        if not saved or saved.get('released') is True:return
        require(saved.get('operation_id')==self.operation and saved.get('sha256')==hashlib.sha256(self.api_restart_override_bytes()).hexdigest(),'API restart override ownership changed')
        path=self.api_restart_override_path()
        if path.exists() or path.is_symlink():
            self.verify_api_restart_override(path);path.unlink()
            directory=os.open(path.parent,os.O_RDONLY);os.fsync(directory);os.close(directory)
        # /run may have been cleared by a reboot, or unlink may have completed
        # before an interrupted reply. Absence still requires effective-policy verification.
        self.run(['systemctl','--user','daemon-reload'])
        require(self.run(['systemctl','--user','show','indeedhub-api.service','--property=Restart','--value']).decode().strip()==saved['original_policy'],'Original API restart policy was not restored; hold retained')
        saved['released']=True;self.save()
    def signal_legacy_api(self, member):
        stopped=self.record['stopped'][member['name']]
        if stopped.get('api_signal'):
            require(stopped['api_signal'].get('acknowledged') is True,'Unacknowledged legacy API signal; hold retained')
            return
        image=self.api_recovery_identity(member)
        require(datetime.datetime.fromisoformat(image['Created'].replace('Z','+00:00')).timestamp()<=stopped['intent_at'],'API recovery image was not captured before stop')
        actual=self.inspect(member['name']);require(actual['Id']==member['container_id'] and actual['Image']==member['image_id'] and actual['State']['Running'],'Original API changed before signal')
        self.ensure_api_restart_override()
        binding=self.api_queue_binding(member,actual,image)
        queue=self.queue('status');prefix=queue.get('prefix');before=self.observe_empty_redis_queue(member,prefix,binding)
        self.legacy_api_idle()
        proof=json.loads(self.run(['podman','exec',member['container_id'],'node','-e',API_PROCESS_SCRIPT,'probe']))
        require(valid_api_process_proof(proof),'Incomplete API process identity proof')
        intent={'operation_id':self.operation,'container_id':member['container_id'],'proof':proof,'prefix':prefix,'queue_binding':binding,'before_queue':before,'intent_at':time.time(),'acknowledged':False}
        stopped['api_signal']=intent;self.save()
        ack=json.loads(self.run(['podman','exec',member['container_id'],'node','-e',API_PROCESS_SCRIPT,'signal',json.dumps(proof,separators=(',',':'))]))
        require(ack=={'proof':proof,'signal':'SIGTERM','acknowledged':True},'Legacy API signal acknowledgement changed')
        intent['acknowledged']=True;intent['acknowledged_at']=time.time();self.save()
        deadline=time.monotonic()+30
        while self.run(['podman','ps','--no-trunc','--filter','id='+member['container_id'],'--format','{{.ID}}']).strip():
            require(time.monotonic()<deadline,'Legacy API did not terminate after child signal');time.sleep(0.2)
        self.require_api_stopped(member)
    def legacy_api_wrapper_termination(self, member, properties):
        self.api_recovery_identity(member)
        stopped=self.record['stopped'][member['name']];signal=stopped.get('api_signal',{})
        require(signal.get('operation_id')==self.operation and signal.get('container_id')==member['container_id'] and signal.get('acknowledged') is True and signal.get('intent_at',0)>=stopped['intent_at'],'Legacy API signal proof missing')
        require(valid_empty_business(self.record.get('legacy_api_empty_state')),'Legacy API empty-business proof missing')
        require(valid_api_process_proof(signal.get('proof')) and valid_empty_queue(signal.get('before_queue')) and type(signal.get('acknowledged_at')) in (int,float) and signal['acknowledged_at']>=signal['intent_at'],'Legacy API durable signal proof incomplete')
        state=dict(line.split('=',1) for line in properties.splitlines() if '=' in line)
        require(state.get('ActiveState')=='failed' and state.get('SubState')=='failed' and state.get('ExecMainStatus')=='1' and state.get('Result')=='exit-code','Unrecognized API wrapper exit')
        require(not self.run(['podman','ps','--no-trunc','--filter','id='+member['container_id'],'--format','{{.ID}}']).strip(),'Legacy API is still running')
        self.require_api_stopped(member)
        require(isinstance(signal.get('queue_binding'),dict),'API queue binding proof missing')
        after=self.observe_empty_redis_queue(member,signal['prefix'],signal['queue_binding'])
        return {'classification':'empty-business-legacy-api-child-signal-termination','graceful':False,'completed_work_claim':False,'process_dead':True,'signal':signal,'after_queue':after}
    def legacy_idle_worker_termination(self, member, properties):
        # Compatibility only for the observed original Node-as-PID1 worker.
        # A timeout/SIGKILL is never renamed graceful or completed work.
        require(member['name']=='indeedhub-ffmpeg','Forced termination is not allowed for this writer')
        self.holds();self.fence_matches()
        before=self.record.get('last_queue_counts')
        require(self.record.get('ingress_closed') is True and self.record.get('stopped',{}).get('indeedhub',{}).get('confirmed') is True,'Legacy worker ingress is not closed')
        require(self.record.get('queue_pause_confirmed') is True and valid_queue_counts(before) and all(v==0 for v in before.values()),'Legacy worker queue was not proven completely empty')
        stopped=self.record['stopped'][member['name']]
        require(stopped.get('container_id')==member['container_id'] and type(stopped.get('intent_at')) in (int,float),'Legacy worker stop intent changed')
        state=dict(line.split('=',1) for line in properties.splitlines() if '=' in line)
        require(state.get('ActiveState') in ('inactive','failed') and state.get('SubState') in ('dead','failed') and state.get('ExecMainStatus')=='137','Legacy worker process death is not established')
        runtime=json.loads((self.data/'update-transactions'/'supervised'/(self.operation+'.json')).read_text())
        require(runtime.get('id')==self.operation and runtime.get('phase') in ('Editing','Restoring') and runtime.get('target_startup_began') is False,'Legacy worker operation changed')
        records=[m for m in runtime['members'] if m['original']['name']==member['name']]
        require(len(records)==1,'Ambiguous legacy worker recovery identity')
        original=records[0]['original'];recovery=records[0]['recovery_image']
        require(original['container_id']==member['container_id'] and original['image'].removeprefix('sha256:')==member['image_id'].removeprefix('sha256:') and original['config_sha256']==member['config_sha256'] and hashlib.sha256(original['body'].encode()).hexdigest()==member['unit_sha256'],'Legacy worker original identity changed')
        require(recovery['source_container_id']==member['container_id'] and recovery['operation_id']==self.operation,'Legacy worker recovery image belongs to another operation')
        rows=json.loads(self.run(['podman','image','inspect',recovery['image']]))
        require(len(rows)==1 and rows[0]['Id'].removeprefix('sha256:')==recovery['image'].removeprefix('sha256:'),'Legacy worker recovery image changed')
        image=rows[0];config=image['Config']
        require(config.get('Cmd')==['node','dist/ffmpeg-worker/worker.js'] and config.get('Entrypoint')==['docker-entrypoint.sh'],'Unrecognized legacy worker command')
        created=datetime.datetime.fromisoformat(image['Created'].replace('Z','+00:00')).timestamp()
        require(created<=stopped['intent_at'],'Legacy worker recovery image was not captured before stop')
        source=pathlib.Path(self.run(['systemctl','--user','show',member['name']+'.service','--property=SourcePath','--value']).decode().strip())
        require(source.is_file() and not source.is_symlink() and source.suffix=='.container' and source.stat().st_uid==os.getuid() and sha(source)==member['unit_sha256'],'Legacy worker saved unit changed')
        rows=self.run(['podman','ps','--all','--no-trunc','--filter','id='+member['container_id'],'--format','{{.ID}} {{.State}}']).decode().splitlines()
        require(not rows or rows==[member['container_id']+' exited'],'Legacy worker may still be running')
        after=self.queue('status')
        require(after['paused'] is True and all(v==0 for v in after['counts'].values()),'Legacy worker queue reopened or changed')
        return {'classification':'legacy-idle-worker-forced-termination','graceful':False,'completed_work_claim':False,
                'original_container_id':member['container_id'],'original_image_id':member['image_id'],
                'recovery_image_id':recovery['image'],'unit_sha256':member['unit_sha256'],
                'before_counts':before,'after_counts':after['counts'],'process_dead':True}
    def graceful_stop(self, name):
        # Save the obligation before systemd can remove an AutoRemove container.
        stopped=self.record.setdefault('stopped',{})
        if stopped.get(name,{}).get('confirmed'):return
        member=next(m for m in self.record['original_members'] if m['name']==name)
        if name not in stopped:
            actual=self.inspect(name);require(actual['Id']==member['container_id'] and actual['Image']==member['image_id'],'Original container changed before stop')
            stopped[name]={'intent_at':time.time(),'container_id':actual['Id']};self.save()
        if name=='indeedhub-api':self.signal_legacy_api(member)
        self.run(['systemctl','--user','stop',name+'.service'],timeout=180)
        properties=self.run(['systemctl','--user','show',name+'.service','--property=ActiveState,SubState,Result,ExecMainStatus']).decode()
        # --rm removes inspect state. Require a persisted Podman died event for
        # this exact original ID; a forced SIGKILL is never called completed work.
        events=self.run(['podman','events','--stream=false','--since',str(int(stopped[name]['intent_at'])-1),'--filter','container='+member['container_id'],'--filter','event=died','--format','json']).decode().splitlines()
        matching=[json.loads(line) for line in events if line.strip()]
        matching=[event for event in matching if event.get('ID',event.get('id'))==member['container_id']]
        require(matching,'Original process exit evidence unavailable; hold retained')
        code=matching[-1].get('ContainerExitCode',matching[-1].get('containerExitCode'))
        idle_worker = name=='indeedhub-ffmpeg' and self.record.get('queue_pause_confirmed') is True and valid_queue_counts(self.record.get('last_queue_counts')) and self.record['last_queue_counts']['active']==0
        empty_api = name=='indeedhub-api' and self.record.get('legacy_api_empty_state') is not None
        forced=self.legacy_idle_worker_termination(member,properties) if str(code)=='137' else None
        if name=='indeedhub-api' and str(code)=='1':forced=self.legacy_api_wrapper_termination(member,properties)
        require(forced is not None or ('ActiveState=inactive' in properties and 'Result=success' in properties),'Service did not stop successfully')
        require(forced is not None or str(code)=='0' or (str(code)=='143' and (idle_worker or empty_api)),'Original process did not exit cleanly; active work is not claimed completed')
        classification=forced['classification'] if forced else (('idle-worker-terminated-after-queue-drain' if idle_worker else 'empty-business-store-legacy-api-terminated') if str(code)=='143' else 'clean-process-exit')
        if forced:stopped[name]['legacy_idle_termination']=forced
        stopped[name].update(confirmed=True,exit_code=int(code),classification=classification,confirmed_at=time.time());self.save()
    def volume_sources(self):
        expected=VOLUMES
        rows=json.loads(self.run(['podman','volume','inspect',*expected]))
        require({row['Name'] for row in rows}==set(expected),'Persistent volume scope changed')
        return {row['Name']:row['Mountpoint'] for row in rows}
    def database_commitments(self, container="indeedhub-postgres"):
        raw=self.run(['podman','exec','-i',container,'psql','-XqAt','--set=ON_ERROR_STOP=1','-U','indeedhub','-d','indeedhub'],timeout=300,input_bytes=DB_COMMITMENTS_SQL.encode())
        rows=[json.loads(line) for line in raw.decode().splitlines() if line.strip()]
        tables={};migrations=None
        for row in rows:
            if 'migration_rows' in row:
                require(migrations is None,'Duplicate database migration observation');migrations=row['migration_rows']
            else:
                name=row.pop('table');require(name not in tables and re.fullmatch('[a-zA-Z_][a-zA-Z0-9_]*',name),'Invalid database table observation');tables[name]=row
        require(tables and 'migrations' in tables and isinstance(migrations,list),'Database compatibility observation incomplete')
        return {'operation_id':self.operation,'tables':tables,'migrations':migrations}
    def verify_restored_data(self):
        baseline=self.record.get('database_before')
        require(baseline and baseline.get('operation_id')==self.operation,'Data compatibility baseline missing')
        current=self.database_commitments();proof=verify_database_compatibility(baseline,current)
        self.record['recovery_data_verification']={'operation_id':self.operation,'checked_at':time.time(),'before_sha256':hashlib.sha256(json.dumps(baseline,sort_keys=True).encode()).hexdigest(),'after_sha256':hashlib.sha256(json.dumps(current,sort_keys=True).encode()).hexdigest(),**proof}
        self.record['recovery_data_verified']=True;self.save()
    def backup(self):
        if self.record.get('backup_complete'):return
        if 'database_before' not in self.record:
            self.record['database_before']=self.database_commitments();self.save()
        sources=self.volume_sources();self.record['volume_sources']=sources;self.save()
        backup=self.root/'backup';backup.mkdir(mode=0o700,exist_ok=True)
        if 'database.dump' not in self.record.setdefault('artifacts',{}):
            path=backup/'database.dump.partial'
            with path.open('wb') as output:self.run(['podman','exec','indeedhub-postgres','pg_dump','-U','indeedhub','-d','indeedhub','--format=custom','--no-owner','--no-acl'],timeout=300,output=output);output.flush();os.fsync(output.fileno())
            final=backup/'database.dump';os.replace(path,final);self.record['artifacts']['database.dump']={'bytes':final.stat().st_size,'sha256':sha(final)};self.save()
        # Redis stop flushes persisted queue state; its clean process exit is
        # checked exactly as every other service. SQLite WAL is archived with DB.
        for name in ('indeedhub-minio','indeedhub-redis','indeedhub-relay','indeedhub-postgres'):self.graceful_stop(name)
        for volume,source in sources.items():
            name=volume+'.tar'
            if name in self.record['artifacts']:continue
            require(pathlib.Path(source).is_absolute() and source.endswith('/_data'),'Invalid volume mountpoint')
            available=shutil.disk_usage(backup).free
            measured=int(self.run(['podman','unshare','du','-sb',source]).decode().split()[0])
            require(available>measured+512*1024*1024,'Insufficient durable backup space')
            partial=backup/(name+'.partial')
            with partial.open('wb') as output:self.run(['podman','unshare','tar','--xattrs','--acls','--numeric-owner','-C',source,'-cpf','-','.'],timeout=1800,output=output);output.flush();os.fsync(output.fileno())
            final=backup/name;os.replace(partial,final);self.record['artifacts'][name]={'bytes':final.stat().st_size,'sha256':sha(final)};self.save()
        self.record['backup_complete']=True;self.record['phase']='Drained';self.save()
    def acquire(self, members, recovery=False):
        members=validate_members(members);self.holds()
        if self.record:require(self.record['original_members']==members,'Original operation terms changed')
        else:
            self.record={'operation_id':self.operation,'original_members':members,'phase':'Prepared','created_at':time.time()};self.save()
        require(self.record['phase']!='Released','Completed maintenance must not be reacquired')
        if not recovery and not self.record.get('originals_validated'):
            for member in members:
                actual=self.inspect(member['name']);require(actual['Id']==member['container_id'] and actual['Image']==member['image_id'],'Original member changed')
                bindings=actual['HostConfig'].get('PortBindings') or {}
                if member['name']=='indeedhub':require(bindings=={'7777/tcp':[{'HostIp':'127.0.0.1','HostPort':'7778'}]},'Unsupported direct frontend exposure')
                else:require(not bindings,'Unsupported direct writer exposure')
                source=pathlib.Path(self.run(['systemctl','--user','show',member['name']+'.service','--property=SourcePath','--value']).decode().strip())
                require(source.is_file() and not source.is_symlink() and source.suffix=='.container','Original unit source missing')
                require(source.stat().st_uid==os.getuid() and sha(source)==member['unit_sha256'],'Original unit source changed')
            self.record['originals_validated']=True;self.save()
        self.close_ingress()
        if recovery:
            runtime=json.loads((self.data/'update-transactions'/'supervised'/(self.operation+'.json')).read_text())
            require(runtime.get('phase')=='Restoring' and type(runtime.get('target_startup_began')) is bool,'Durable explicit restoring obligation required')
            self.record['phase']='Recovering';self.record['target_startup_began']=runtime['target_startup_began'];self.save()
            return {'operation_id':self.operation,'state':'recovering'}
        if not self.record.get('stopped',{}).get('indeedhub-api',{}).get('confirmed'):
            self.pause_queue()
            self.graceful_stop('indeedhub')
            deadline=time.monotonic()+300
            while True:
                state=self.queue('status');require(state['paused'],'Worker admission reopened')
                self.record['last_queue_counts']=state['counts'];self.save()
                if state['counts']['active']==0:break
                require(time.monotonic()<deadline,'Transcodes still active; retained job state, no forced completion')
                time.sleep(1)
            self.graceful_stop('indeedhub-ffmpeg');self.legacy_api_idle();self.graceful_stop('indeedhub-api')
        self.backup();self.verify_database_backup();self.verify_volume_backups();self.verify();return {'operation_id':self.operation,'state':'drained'}
    def backup_restore_terms(self):
        baseline=self.record.get('database_before')
        require(baseline and baseline.get('operation_id')==self.operation,'Backup database baseline missing')
        postgres=next(m for m in validate_members(self.record['original_members']) if m['name']=='indeedhub-postgres')
        return {'operation_id':self.operation,'dump_sha256':self.record['artifacts']['database.dump']['sha256'],
                'baseline_sha256':hashlib.sha256(json.dumps(baseline,sort_keys=True).encode()).hexdigest(),
                'image_id':postgres['image_id']}
    def cleanup_restore_fixture(self):
        fixture=self.record.get('restore_fixture')
        if not fixture:return
        name=fixture['name']
        require(bool(re.fullmatch('archy-backup-restore-[0-9a-f]{32}',name)),'Invalid restore fixture name')
        ids=self.run(['podman','ps','--all','--no-trunc','--filter','name=^'+name+'$','--format','{{.ID}}']).decode().split()
        require(len(ids)<=1,'Ambiguous restore fixture')
        if ids:
            actual=self.inspect(ids[0])
            require(actual['Name']==name and actual['Image'].removeprefix('sha256:')==fixture['image_id'] and
                    actual['Config'].get('Labels',{}).get('io.archipelago.backup.operation')==self.operation,
                    'Restore fixture ownership changed')
            require(not actual.get('Mounts'),'Restore fixture unexpectedly mounts external storage')
            self.run(['podman','rm','--force',actual['Id']],timeout=90)
        del self.record['restore_fixture'];self.save()
    def verify_database_backup(self):
        # A valid digest only proves unchanged bytes, not a usable PostgreSQL
        # backup. Restore the exact fresh dump before allowing target startup.
        self.holds();self.fence_matches();self.verify_artifacts()
        terms=self.backup_restore_terms()
        self.cleanup_restore_fixture()
        if self.record.get('backup_restore_verified'):
            require(self.record['backup_restore_verified']==terms,'Backup restore proof changed')
            return
        name='archy-backup-restore-'+uuid.uuid4().hex
        self.record['restore_fixture']={'name':name,'image_id':terms['image_id']};self.save()
        try:
            # No published ports, network, mounted volumes, or registry access.
            # PGDATA is private disposable container storage, not RAM or live data.
            identifier=self.run(['podman','create','--pull=never','--network=none','--image-volume=ignore',
                '--name',name,'--label','io.archipelago.backup.operation='+self.operation,
                '-e','POSTGRES_HOST_AUTH_METHOD=trust','-e','POSTGRES_USER=indeedhub',
                '-e','POSTGRES_DB=indeedhub','-e','PGDATA=/var/lib/postgresql/data/restore-check',
                'sha256:'+terms['image_id']]).decode().strip()
            require(bool(re.fullmatch('[0-9a-f]{64}',identifier)),'Invalid restore fixture identity')
            actual=self.inspect(identifier)
            require(not actual.get('Mounts'),'Restore fixture unexpectedly mounts external storage')
            self.run(['podman','start',identifier])
            # The image bootstrap server accepts Unix sockets before it exits;
            # TCP readiness waits for the final server, avoiding interrupted restore.
            deadline=time.monotonic()+90
            while True:
                try:
                    self.run(['podman','exec',identifier,'pg_isready','-h','127.0.0.1','-U','indeedhub','-d','indeedhub'],timeout=10)
                    break
                except subprocess.CalledProcessError:
                    require(time.monotonic()<deadline,'Backup restore database did not become ready')
                    time.sleep(0.5)
            with (self.root/'backup'/'database.dump').open('rb') as source:
                self.run(['podman','exec','-i',identifier,'pg_restore','--exit-on-error','--no-owner','--no-acl',
                          '-U','indeedhub','-d','indeedhub'],timeout=1800,input_file=source)
            restored=self.database_commitments(identifier)
            require(restored==self.record['database_before'],'Backup restore differs from captured database')
        finally:
            self.cleanup_restore_fixture()
        self.record['backup_restore_verified']=terms;self.save()
    def volume_restore_terms(self):
        return {'operation_id':self.operation,'archives':{
            name:self.record['artifacts'][name]['sha256'] for name in (v+'.tar' for v in VOLUMES)}}
    def cleanup_volume_fixture(self):
        name=self.record.get('volume_restore_fixture')
        if name is None:return
        require(bool(re.fullmatch('volume-restore-[0-9a-f]{32}',name)),'Invalid volume restore fixture')
        path=self.root/name
        if path.exists() or path.is_symlink():
            require(path.is_dir() and not path.is_symlink() and path.stat().st_uid==os.getuid(),'Volume restore ownership changed')
            owner=path/'owner'
            require(owner.is_file() and not owner.is_symlink() and owner.read_text()==self.operation,'Volume restore ownership changed')
            self.run(['podman','unshare','rm','-rf','--',str(path/'payload')],timeout=1800)
            roundtrip=path/'roundtrip.tar'
            if roundtrip.exists():
                require(roundtrip.is_file() and not roundtrip.is_symlink(),'Volume restore output changed')
                roundtrip.unlink()
            owner.unlink();path.rmdir()
        del self.record['volume_restore_fixture'];self.save()
    def verify_volume_backups(self):
        # Restore each entire volume to fresh owned storage, then have GNU tar
        # compare its bytes, links, ownership, modes and metadata to the archive.
        # Live volumes are neither mounted nor written by this verification.
        self.holds();self.fence_matches();self.verify_artifacts()
        terms=self.volume_restore_terms();self.cleanup_volume_fixture()
        if self.record.get('volume_restore_verified'):
            require(self.record['volume_restore_verified']==terms,'Volume restore proof changed');return
        for volume in VOLUMES:
            archive=self.root/'backup'/(volume+'.tar')
            measured=validate_volume_archive(archive)
            require(shutil.disk_usage(self.root).free>measured*2+512*1024*1024,'Insufficient volume restore space')
            name='volume-restore-'+uuid.uuid4().hex;path=self.root/name
            path.mkdir(mode=0o700)
            with (path/'owner').open('x') as owner:
                owner.write(self.operation);owner.flush();os.fsync(owner.fileno())
            self.record['volume_restore_fixture']=name;self.save()
            try:
                (path/'payload').mkdir(mode=0o700)
                self.run(['podman','unshare','tar','--xattrs','--acls','--numeric-owner',
                    '--same-owner','--same-permissions','-C',str(path/'payload'),'-xpf',str(archive)],timeout=1800)
                self.run(['podman','unshare','tar','--xattrs','--acls','--numeric-owner',
                    '-C',str(path/'payload'),'-df',str(archive)],timeout=1800)
                # GNU tar --compare omits extended attributes. Re-archive the
                # restored tree and compare numeric ownership, modes, ACLs and
                # xattrs explicitly (normalizing hardlink traversal order).
                roundtrip=path/'roundtrip.tar'
                with roundtrip.open('xb') as output:
                    self.run(['podman','unshare','tar','--xattrs','--acls','--numeric-owner',
                        '-C',str(path/'payload'),'-cpf','-','.'],timeout=1800,output=output)
                require(volume_archive_metadata(archive)==volume_archive_metadata(roundtrip),'Restored volume metadata differs from backup')
            finally:self.cleanup_volume_fixture()
        self.verify_artifacts()
        self.record['volume_restore_verified']=terms;self.save()
    def verify_artifacts(self):
        expected_artifacts={'database.dump',*(volume+'.tar' for volume in VOLUMES)}
        require(set(self.record.get('artifacts',{}))==expected_artifacts,'Backup artifact inventory incomplete or unexpected')
        for name,record in self.record['artifacts'].items():
            path=self.root/'backup'/name;require(path.is_file() and not path.is_symlink() and path.stat().st_size==record['bytes'],'Backup artifact missing or changed')
            require(sha(path)==record['sha256'],'Backup artifact checksum changed')
    def verify(self):
        self.holds();self.fence_matches()
        if self.record and self.record.get('phase')=='Recovering':
            runtime=json.loads((self.data/'update-transactions'/'supervised'/(self.operation+'.json')).read_text())
            require(runtime.get('phase')=='Restoring','Recovery ownership changed')
            return {'operation_id':self.operation,'state':'held'}
        require(self.record and self.record.get('backup_complete'),'Drain not complete')
        require(self.record['phase'] in ('Drained','Released'),'Invalid maintenance phase')
        # Verification remains possible when API/storage endpoints are stopped.
        # The native adapter separately validates target/original runtime identity.
        for name in NAMES:require(self.record.get('stopped',{}).get(name,{}).get('confirmed'),'Original writer stop evidence missing')
        self.verify_artifacts()
        require(self.record.get('backup_restore_verified')==self.backup_restore_terms(),'Fresh database backup restore is not verified')
        require(self.record.get('volume_restore_verified')==self.volume_restore_terms(),'Fresh volume backup restore is not verified')
        return {'operation_id':self.operation,'state':'held'}
    def release(self, outcome):
        require(outcome in ('committed','restored','aborted'),'Invalid release outcome')
        if self.record is None and outcome=='aborted':
            runtime=json.loads((self.data/'update-transactions'/'supervised'/(self.operation+'.json')).read_text())
            require(runtime.get('phase')=='Aborted' and runtime.get('target_startup_began') is False,'Untouched abort evidence required')
            if self.fence.exists():
                require(not self.fence.is_symlink() and self.fence.read_text()!=self.operation,'Matching fence without journal requires recovery')
            return {'operation_id':self.operation,'state':'released'}
        require(self.record is not None,'Unknown maintenance operation')
        if self.record['phase']=='Released':
            require(self.record.get('outcome')==outcome,'Maintenance outcome changed')
            if self.fence.exists():
                self.fence_matches();self.fence.unlink()
            return {'operation_id':self.operation,'state':'released'}
        self.holds();self.fence_matches()
        runtime=json.loads((self.data/'update-transactions'/'supervised'/(self.operation+'.json')).read_text());phase=runtime['phase']
        require(phase=={'committed':'Committed','restored':'Restored','aborted':'Restored'}[outcome],'Runtime outcome not durably verified')
        # Restoring old runtime over a changed database is not sufficient to open
        # admission. Node must verify same-schema/additive migration compatibility.
        if outcome!='committed':
            require(type(runtime.get('target_startup_began')) is bool,'Target-start obligation unavailable')
            if runtime['target_startup_began']:
                self.verify_restored_data()
            else:
                self.record['rollback_data_claim']='No target startup/migration began; only original runtime restored.'

        self.release_api_restart_override()
        if not self.record.get('queue_was_paused',True):
            state=self.queue('resume');require(not state['paused'],'Could not restore queue admission')
        self.record['phase']='Released';self.record['outcome']=outcome;self.save()
        self.fence.unlink();fd=os.open(self.fence.parent,os.O_RDONLY);os.fsync(fd);os.close(fd)
        return {'operation_id':self.operation,'state':'released'}
def main():
    os.umask(0o077);require(len(sys.argv)==2 and sys.argv[1] in ('acquire','verify','release'),'Unsupported maintenance action')
    raw=sys.stdin.buffer.read(65537);require(len(raw)<=65536,'Maintenance request too large');request=json.loads(raw)
    allowed={'operation_id','original_members'} if sys.argv[1]=='acquire' else {'operation_id','outcome'} if sys.argv[1]=='release' else {'operation_id'}
    require(set(request) in (allowed, allowed|{'recovery'}) if sys.argv[1]=='acquire' else set(request)==allowed,'Unexpected maintenance fields')
    if 'recovery' in request:require(type(request['recovery']) is bool,'Invalid recovery flag')
    require(os.getuid()==1000,'Expected node service user');fd=int(os.environ['ARCHY_UPDATE_LOCK_FD']);actual=os.fstat(fd);expected=(DATA/'update-transactions'/'lock').stat();require((actual.st_dev,actual.st_ino)==(expected.st_dev,expected.st_ino),'Inherited lifecycle lock is not the expected file')
    controller=Controller(DATA,request['operation_id'],fd)
    result=controller.acquire(request['original_members'],request.get('recovery',False)) if sys.argv[1]=='acquire' else controller.verify() if sys.argv[1]=='verify' else controller.release(request['outcome'])
    encoded=json.dumps(result);require(len(encoded)<=4096,'Maintenance response exceeds bound');print(encoded)
if __name__=='__main__':
    try:main()
    except Exception as error:
        # Command/env details remain in private journal, never RPC/UI stdout.
        print(json.dumps({'error':'Maintenance remains held; inspect its private operation journal','reason':type(error).__name__}),file=sys.stderr);sys.exit(1)
