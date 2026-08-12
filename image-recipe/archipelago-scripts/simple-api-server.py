#!/usr/bin/env python3
"""
Simple API server for Archipelago Web UI
Serves static files and handles basic API endpoints
"""

import http.server
import socketserver
import json
import os
from urllib.parse import urlparse, parse_qs

PORT = 80
WEB_DIR = None

# Find web UI directory
for path in ['/opt/archipelago/web-ui', '/run/live/medium/archipelago/web-ui', '/lib/live/mount/medium/archipelago/web-ui']:
    if os.path.isdir(path):
        WEB_DIR = path
        break

if not WEB_DIR:
    print("Web UI directory not found!")
    exit(1)

os.chdir(WEB_DIR)

class ArchipelagoHandler(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        """Handle POST requests with mock responses"""
        content_length = int(self.headers.get('Content-Length', 0))
        post_data = self.rfile.read(content_length) if content_length > 0 else b''
        
        path = urlparse(self.path).path
        
        # Mock API responses
        response = {"success": True}
        
        if '/api/auth' in path or '/auth' in path:
            response = {
                "success": True,
                "token": "mock-token-12345",
                "user": "archipelago"
            }
        elif '/api/setup' in path or '/setup' in path:
            response = {
                "success": True,
                "status": "complete"
            }
        elif '/api/status' in path or '/status' in path:
            response = {
                "success": True,
                "status": "running",
                "version": "0.1.0",
                "hostname": "archipelago"
            }
        elif '/api/apps' in path or '/apps' in path:
            response = {
                "success": True,
                "apps": []
            }
        
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Access-Control-Allow-Origin', '*')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())
    
    def do_OPTIONS(self):
        """Handle CORS preflight"""
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type, Authorization')
        self.end_headers()
    
    def do_GET(self):
        """Handle GET requests - serve files or API"""
        path = urlparse(self.path).path
        
        if path.startswith('/api/'):
            # Mock API GET endpoints
            response = {"success": True}
            
            if '/status' in path:
                response = {
                    "success": True,
                    "status": "running",
                    "version": "0.1.0"
                }
            elif '/apps' in path:
                response = {
                    "success": True,
                    "apps": [
                        {"id": "bitcoin", "name": "Bitcoin Core", "status": "available"},
                        {"id": "lnd", "name": "Lightning (LND)", "status": "available"}
                    ]
                }
            
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Access-Control-Allow-Origin', '*')
            self.end_headers()
            self.wfile.write(json.dumps(response).encode())
        else:
            # Serve static files, default to index.html for SPA routing
            if not os.path.exists(self.translate_path(self.path)) or path == '/':
                self.path = '/index.html'
            super().do_GET()
    
    def log_message(self, format, *args):
        """Quieter logging"""
        if '404' in str(args) or 'error' in str(args).lower():
            print(f"  {args[0]}")

print(f"""
╔═══════════════════════════════════════════════════════════╗
║   🏝️  ARCHIPELAGO WEB UI                                  ║
╚═══════════════════════════════════════════════════════════╝

  Serving from: {WEB_DIR}
  
  🌐 Open in your browser: http://localhost:{PORT}
  
  Press Ctrl+C to stop
""")

with socketserver.TCPServer(("0.0.0.0", PORT), ArchipelagoHandler) as httpd:
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
