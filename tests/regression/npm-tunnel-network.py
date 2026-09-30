#!/usr/bin/env python3
"""Real nftables routing check. Run: sudo unshare --net python3 <this file>.
Never runs in the host network namespace; creates no persistent namespaces.
"""
import importlib.util
import os
from pathlib import Path
import socket
import subprocess
import threading

assert os.geteuid() == 0
assert os.readlink('/proc/self/ns/net') != os.readlink('/proc/1/ns/net'), 'requires isolated network namespace'
repo=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('fixture',repo/'scripts/tests/test_repair_npm_tunnel.py')
f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
def run(*args,input=None):
    return subprocess.run(args,input=input,text=True,capture_output=True,check=True,timeout=10).stdout
run('ip','link','set','lo','up')
peer=subprocess.Popen(['unshare','--net','sleep','60'])
try:
    import time
    for _ in range(100):
        if os.readlink(f'/proc/{peer.pid}/ns/net')!=os.readlink('/proc/self/ns/net'): break
        time.sleep(.02)
    else: raise AssertionError('peer namespace did not start')
    run('ip','link','add','wg-web','type','veth','peer','name','wgpeer')
    run('ip','link','set','wgpeer','netns',str(peer.pid))
    run('ip','addr','add','10.77.0.2/30','dev','wg-web')
    run('ip','link','set','wg-web','up')
    prefix=('nsenter','-t',str(peer.pid),'-n')
    run(*prefix,'ip','addr','add','10.77.0.1/30','dev','wgpeer')
    run(*prefix,'ip','link','set','wgpeer','up')
    run(*prefix,'ip','link','set','lo','up')
    listeners=[]
    for address,reply in [(('10.77.0.2',18081),b'NPM'),(('0.0.0.0',18080),b'LND')]:
        listener=socket.socket();listener.bind(address);listener.listen();listeners.append(listener)
        def serve(sock=listener,data=reply):
            connection,_=sock.accept()
            with connection: connection.sendall(data)
        threading.Thread(target=serve,daemon=True).start()
    run('nft','-f','-',input=f.RULES)
    _,rules,_=f.m.plan(f.DROP,f.RULES)
    transaction='delete table inet web_tunnel\n'+rules
    run('nft','--check','-f','-',input=transaction)
    run('nft','-f','-',input=transaction)
    result=run(*prefix,'python3','-c',"import socket; s=socket.create_connection(('10.77.0.2',18080),3); print(s.recv(10).decode())")
    assert result.strip()=='NPM',result
    with socket.create_connection(('127.0.0.1',18080),3) as connection:
        assert connection.recv(10)==b'LND'
    print('PASS: original peer HTTP port reaches NPM; local LND REST port remains separate')
finally:
    peer.terminate();peer.wait(timeout=5)
