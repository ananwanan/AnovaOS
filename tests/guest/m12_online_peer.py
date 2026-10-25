# SPDX-License-Identifier: Apache-2.0
# Serve real sparse-registry archives and Git smart HTTP over strict, independently tested TLS.
# All generated keys and repository inputs stay on the host; no compiler is exposed by this peer.
import argparse
import gzip
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import ssl
import subprocess
import tarfile
import threading
import urllib.parse

parser = argparse.ArgumentParser()
parser.add_argument('--root', required=True, type=Path)
parser.add_argument('--prepare', action='store_true')
args = parser.parse_args()
root = args.root.resolve()
root.mkdir(parents=True, exist_ok=True)


def run(*command, **kwargs):
    return subprocess.check_output(command, stderr=subprocess.PIPE, **kwargs)


if args.prepare:
    # A small real crate makes cold downloads independent of public-index churn.
    source = {
        'Cargo.toml': '# SPDX-License-Identifier: Apache-2.0\n'
                      '# Define the network-downloaded Rust acceptance dependency.\n'
                      '[package]\nname="m12-net-dep"\nversion="0.1.0"\n'
                      'edition="2024"\nlicense="Apache-2.0"\n',
        'src/lib.rs': '// SPDX-License-Identifier: Apache-2.0\n'
                      '//! Return the independently downloaded registry contribution.\n'
                      'pub fn answer() -> u32 { 21 }\n',
    }
    tar_bytes = io.BytesIO()
    with tarfile.open(fileobj=tar_bytes, mode='w') as archive:
        for name, text in sorted(source.items()):
            data = text.encode()
            member = tarfile.TarInfo('m12-net-dep-0.1.0/' + name)
            member.size, member.mode, member.mtime = len(data), 0o644, 0
            archive.addfile(member, io.BytesIO(data))
    crate = gzip.compress(tar_bytes.getvalue(), mtime=0)
    (root / 'dependency.crate').write_bytes(crate)
    checkout = root / 'source'
    checkout.mkdir(exist_ok=True)
    (checkout / 'src').mkdir(exist_ok=True)
    (checkout / 'Cargo.toml').write_text(source['Cargo.toml'].replace('m12-net-dep', 'm12-git-dep'))
    (checkout / 'src/lib.rs').write_text(source['src/lib.rs'])
    run('git', 'init', '--quiet', '--initial-branch=main', str(checkout))
    run('git', '-C', str(checkout), 'add', '.')
    environment = os.environ.copy()
    environment.update(GIT_AUTHOR_NAME='AnovaOS fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                       GIT_COMMITTER_NAME='AnovaOS fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid',
                       GIT_AUTHOR_DATE='2025-09-18T00:00:00Z',
                       GIT_COMMITTER_DATE='2025-09-18T00:00:00Z')
    if run('git', '-C', str(checkout), 'status', '--porcelain'):
        run('git', '-C', str(checkout), 'commit', '--quiet', '-m', 'Add locked Git dependency',
            env=environment)
    revision = run('git', '-C', str(checkout), 'rev-parse', 'HEAD').decode().strip()
    repository = root / 'dependency.git'
    if not repository.exists():
        run('git', 'clone', '--bare', '--quiet', str(checkout), str(repository))
    else:
        run('git', '-C', str(repository), 'fetch', '--quiet', str(checkout), 'main:main')
    run('openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '2',
        '-subj', '/CN=AnovaOS M12 test CA', '-addext', 'basicConstraints=critical,CA:TRUE',
        '-keyout', str(root / 'ca.key'), '-out', str(root / 'ca.pem'))
    for name, days, san in [('server', '2', 'IP:10.0.2.2,DNS:registry.anova.test'),
                            ('expired', '-1', 'IP:10.0.2.2'),
                            ('wrong-name', '2', 'DNS:wrong.anova.test')]:
        run('openssl', 'req', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=M12 peer',
            '-keyout', str(root / (name + '.key')), '-out', str(root / (name + '.csr')))
        extension = root / (name + '.ext')
        extension.write_text('basicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\n'
                             'subjectAltName=' + san + '\n')
        run('openssl', 'x509', '-req', '-in', str(root / (name + '.csr')), '-CA',
            str(root / 'ca.pem'), '-CAkey', str(root / 'ca.key'), '-CAcreateserial',
            '-days', days, '-extfile', str(extension), '-out', str(root / (name + '.pem')))
    (root / 'fixture.toml').write_text(
        f'crate_sha256 = "{hashlib.sha256(crate).hexdigest()}"\ngit_rev = "{revision}"\n')
    print('Prepared pinned registry archive and Git revision ' + revision, flush=True)
    raise SystemExit(0)

import tomllib
fixture = tomllib.loads((root / 'fixture.toml').read_text())
events_lock = threading.Lock()
corrupt_downloads = 0


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def event(self):
        with events_lock:
            with (root / 'requests.jsonl').open('a') as output:
                output.write(json.dumps({'method': self.command, 'path': self.path,
                                         'port': self.server.server_port}) + '\n')

    def reply(self, data, kind='application/json', status=200, extra=0):
        self.send_response(status)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(data) + extra))
        self.send_header('Connection', 'close')
        self.end_headers()
        self.wfile.write(data)
        self.close_connection = True

    def do_GET(self):
        global corrupt_downloads
        self.event()
        path = urllib.parse.urlsplit(self.path).path
        if path == '/git/dependency.git/info/refs':
            if urllib.parse.urlsplit(self.path).query != 'service=git-upload-pack':
                self.send_error(400)
                return
            banner = b'# service=git-upload-pack\n'
            refs = run('git', 'upload-pack', '--stateless-rpc', '--advertise-refs',
                       str(root / 'dependency.git'))
            self.reply(f'{len(banner)+4:04x}'.encode() + banner + b'0000' + refs,
                       'application/x-git-upload-pack-advertisement')
            return
        parts = path.strip('/').split('/')
        if len(parts) < 3 or parts[0] not in ('ok', 'corrupt', 'truncated', 'timeout'):
            self.send_error(404)
            return
        scenario = parts[0]
        if scenario == 'timeout':
            threading.Event().wait(20)
        if parts[1:] == ['index', 'config.json']:
            self.reply(json.dumps({'dl': f'https://10.0.2.2:{self.server.server_port}/'
                                         f'{scenario}/download/{{crate}}/{{version}}'}).encode())
        elif parts[1:] == ['index', 'm1', '2-', 'm12-net-dep']:
            entry = {'name': 'm12-net-dep', 'vers': '0.1.0', 'deps': [], 'features': {},
                     'cksum': fixture['crate_sha256'], 'yanked': False}
            self.reply((json.dumps(entry) + '\n').encode())
        elif parts[1:] == ['download', 'm12-net-dep', '0.1.0']:
            data = (root / 'dependency.crate').read_bytes()
            if scenario == 'corrupt':
                with events_lock:
                    corrupt_downloads += 1
                    if corrupt_downloads == 1:
                        data += b'corrupted transport fixture'
            self.reply(data, 'application/octet-stream', extra=4096 if scenario == 'truncated' else 0)
        else:
            self.send_error(404)

    def do_POST(self):
        self.event()
        if self.path != '/git/dependency.git/git-upload-pack':
            self.send_error(404)
            return
        length = int(self.headers.get('Content-Length', '0'))
        if length <= 0 or length > 128 * 1024:
            self.send_error(413)
            return
        result = run('git', 'upload-pack', '--stateless-rpc', str(root / 'dependency.git'),
                     input=self.rfile.read(length))
        self.reply(result, 'application/x-git-upload-pack-result')

    def log_message(self, *_):
        pass


servers = []
for port, name in [(18443, 'server'), (18444, 'expired'), (18445, 'wrong-name')]:
    server = http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler)
    server.daemon_threads = True
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(root / (name + '.pem'), root / (name + '.key'))
    server.socket = context.wrap_socket(server.socket, server_side=True)
    servers.append(server)
    threading.Thread(target=server.serve_forever, daemon=True).start()
print('[M12:ONLINE:READY]', flush=True)
threading.Event().wait()
