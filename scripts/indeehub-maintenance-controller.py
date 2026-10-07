#!/usr/bin/env python3
"""Operation-owned legacy IndeeHub maintenance. Called only by supervised updater.
No live execution is part of source qualification. Original writable-layer images
must already be durable. Never unlock ARCHY_UPDATE_LOCK_FD or release another hold.
"""
import datetime, hashlib, json, os, pathlib, re, shutil, subprocess, sys, time, uuid
NAMES = ('indeedhub','indeedhub-api','indeedhub-ffmpeg','indeedhub-minio','indeedhub-postgres','indeedhub-redis','indeedhub-relay')
DATA = pathlib.Path('/var/lib/archipelago')
QUEUE_SCRIPT = r'''const {Queue}=require('bullmq');
(async()=>{const q=new Queue('transcode',{connection:{host:process.env.QUEUE_HOST,port:Number(process.env.QUEUE_PORT||6379),password:process.env.QUEUE_PASSWORD,maxRetriesPerRequest:1}});
try{const action=process.argv[1];if(action==='pause')await q.pause();else if(action==='resume')await q.resume();else if(action!=='status')throw Error('action');
console.log(JSON.stringify({paused:await q.isPaused(),counts:await q.getJobCounts('active','waiting','paused','delayed','failed','completed')}));}
finally{await q.close()}})().catch(()=>process.exit(1));'''
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
class Controller:
    def __init__(self, data, operation, lock_fd, runner=None):
        self.data=pathlib.Path(data);self.operation=operation;self.lock_fd=lock_fd;self.runner=runner
        require(str(uuid.UUID(operation))==operation,'Invalid operation UUID')
        self.root=self.data/'update-transactions'/'indeehub-maintenance'/operation
        self.path=self.root/'journal.json';self.fence=self.data/'app-maintenance'/'indeedhub'
        self.record=json.loads(self.path.read_text()) if self.path.exists() else None
        if self.record:require(self.record['operation_id']==operation,'Maintenance journal changed')
    def save(self): atomic(self.path,self.record)
    def run(self, argv, timeout=30, output=None, input_bytes=None):
        if self.runner:return self.runner(argv,timeout,output)
        self.root.mkdir(mode=0o700,parents=True,exist_ok=True)
        with (self.root/'commands.private.log').open('ab') as errors:
            result=subprocess.run(argv,stdout=output or subprocess.PIPE,stderr=errors,timeout=timeout,check=True,pass_fds=(self.lock_fd,),input=input_bytes)
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
        config=self.run(['sudo','-n','nginx','-T']).decode()
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
        require(type(result.get('paused')) is bool and isinstance(result.get('counts'),dict),'Invalid queue observation')
        for value in result['counts'].values():require(type(value) is int and value>=0,'Invalid job count')
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
        require(self.record.get('queue_pause_confirmed') is True and self.record.get('last_queue_counts',{}).get('active')==0,'Worker queue is not proven idle')
        tables=('projects','contents','payments','shareholders','subscriptions','library_items')
        fields=','.join("'%s',(SELECT count(*) FROM public.%s)"%(name,name) for name in tables)
        sql="SELECT json_build_object("+fields+",'other_active_transactions',(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND state<>'idle'))"
        counts=json.loads(self.run(['podman','exec','indeedhub-postgres','psql','-XAt','-U','indeedhub','-d','indeedhub','-c',sql]))
        require(set(counts)==set(tables)|{'other_active_transactions'},'Legacy API business-state observation incomplete')
        require(all(type(value) is int and value==0 for value in counts.values()),'Legacy API has business work or active transactions; completion cannot be inferred')
        self.record['legacy_api_empty_state']=counts;self.save()
    def graceful_stop(self, name):
        # Save the obligation before systemd can remove an AutoRemove container.
        stopped=self.record.setdefault('stopped',{})
        if stopped.get(name,{}).get('confirmed'):return
        member=next(m for m in self.record['original_members'] if m['name']==name)
        if name not in stopped:
            actual=self.inspect(name);require(actual['Id']==member['container_id'] and actual['Image']==member['image_id'],'Original container changed before stop')
            stopped[name]={'intent_at':time.time(),'container_id':actual['Id']};self.save()
        self.run(['systemctl','--user','stop',name+'.service'],timeout=180)
        properties=self.run(['systemctl','--user','show',name+'.service','--property=ActiveState,SubState,Result,ExecMainStatus']).decode()
        require('ActiveState=inactive' in properties and 'Result=success' in properties,'Service did not stop successfully')
        # --rm removes inspect state. Require a persisted Podman died event for
        # this exact original ID; a forced SIGKILL is never called completed work.
        events=self.run(['podman','events','--stream=false','--since',str(int(stopped[name]['intent_at'])-1),'--filter','container='+member['container_id'],'--filter','event=died','--format','json']).decode().splitlines()
        matching=[json.loads(line) for line in events if line.strip()]
        matching=[event for event in matching if event.get('ID',event.get('id'))==member['container_id']]
        require(matching,'Original process exit evidence unavailable; hold retained')
        code=matching[-1].get('ContainerExitCode',matching[-1].get('containerExitCode'))
        idle_worker = name=='indeedhub-ffmpeg' and self.record.get('queue_pause_confirmed') is True and self.record.get('last_queue_counts',{}).get('active')==0
        empty_api = name=='indeedhub-api' and self.record.get('legacy_api_empty_state') is not None
        require(str(code)=='0' or (str(code)=='143' and (idle_worker or empty_api)),'Original process did not exit cleanly; active work is not claimed completed')
        classification=('idle-worker-terminated-after-queue-drain' if idle_worker else 'empty-business-store-legacy-api-terminated') if str(code)=='143' else 'clean-process-exit'
        stopped[name].update(confirmed=True,exit_code=int(code),classification=classification,confirmed_at=time.time());self.save()
    def volume_sources(self):
        expected=['indeedhub-minio-data','indeedhub-postgres-data','indeedhub-redis-data','indeedhub-relay-data']
        rows=json.loads(self.run(['podman','volume','inspect',*expected]))
        require({row['Name'] for row in rows}==set(expected),'Persistent volume scope changed')
        return {row['Name']:row['Mountpoint'] for row in rows}
    def database_commitments(self):
        raw=self.run(['podman','exec','-i','indeedhub-postgres','psql','-XqAt','--set=ON_ERROR_STOP=1','-U','indeedhub','-d','indeedhub'],timeout=300,input_bytes=DB_COMMITMENTS_SQL.encode())
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
                if state['counts'].get('active',0)==0:break
                require(time.monotonic()<deadline,'Transcodes still active; retained job state, no forced completion')
                time.sleep(1)
            self.graceful_stop('indeedhub-ffmpeg');self.legacy_api_idle();self.graceful_stop('indeedhub-api')
        self.backup();self.verify();return {'operation_id':self.operation,'state':'drained'}
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
        for name,record in self.record['artifacts'].items():
            path=self.root/'backup'/name;require(path.is_file() and not path.is_symlink() and path.stat().st_size==record['bytes'],'Backup artifact missing or changed')
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
