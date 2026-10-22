# SPDX-License-Identifier: Apache-2.0
# Serve deterministic HTTPS test bodies over loopback; fixture keys never enter the guest image.
import argparse
import http.server
import pathlib
import ssl
import threading

parser = argparse.ArgumentParser()
parser.add_argument('--root', required=True)
parser.add_argument('--certificates', required=True)
args = parser.parse_args()
root = pathlib.Path(args.root)
certificates = pathlib.Path(args.certificates)

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.0'

    def do_GET(self):
        if self.path == '/hello':
            data = b'M10-TLS'
        elif self.path == '/demo-2.apkg':
            data = (root / 'demo-2.apkg').read_bytes()
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass

servers = []
for port, certificate in [(18083, 'server.pem'), (18085, 'expired.pem')]:
    server = http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler)
    server.daemon_threads = True
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.maximum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(certificates / certificate, certificates / 'server.key')
    server.socket = context.wrap_socket(server.socket, server_side=True)
    servers.append(server)
    threading.Thread(target=server.serve_forever, daemon=True).start()
print('[M10:HTTPS:READY]', flush=True)
threading.Event().wait()
