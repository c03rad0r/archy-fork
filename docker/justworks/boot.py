"""Run the app adapter and isolated Business service with shared lifecycle."""
import os
import subprocess
import time
children = []
try:
    children.append(subprocess.Popen(['python3', 'business/server.py'], env={**os.environ,
        'JWB_HOST':'127.0.0.1', 'JWB_PORT':'4310', 'JWB_BASE_PATH':'/business-demo',
        'JWB_DATA_DIR':'/data/business', 'JWB_AUTH_MODE':'enforce',
        'JWB_COOKIE_SECURE':'0', 'JWB_ARCHIPELAGO':'1'}))
    children.append(subprocess.Popen(['python3', 'server.py']))
    while all(child.poll() is None for child in children): time.sleep(0.5)
finally:
    for child in children:
        if child.poll() is None: child.terminate()
    for child in children: child.wait()
raise SystemExit(1)
