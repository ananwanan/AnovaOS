# SPDX-License-Identifier: Apache-2.0
# Verify official musl compiler inputs and stage an isolated, inventoried guest Rust sysroot.
import hashlib
import fcntl
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile

root = Path(__file__).resolve().parents[2]
work = root/'build/rust'
work.mkdir(parents=True, exist_ok=True)
lock = (work/'prepare.lock').open('a')
fcntl.flock(lock, fcntl.LOCK_EX)
config = json.loads((root/'ports/rust/inputs.json').read_text())
stage = work/'stage/rs'

def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

def inventory():
    return ''.join(digest(p)+'  /rs/'+str(p.relative_to(stage))+'\n'
                   for p in sorted(stage.rglob('*')) if p.is_file())

recipe = hashlib.sha256(b''.join((root/name).read_bytes() for name in [
    'ports/rust/build.py', 'ports/rust/inputs.json', 'ports/gcc/inputs.json',
    'docs/third-party/m12.md'])).hexdigest()+'\n'
stamp = work/'recipe.sha256'
if sys.argv[1:] == ['--verify']:
    if not stamp.exists() or stamp.read_text() != recipe:
        raise RuntimeError('Rust recipe changed; run cargo xtask rust-build')
    if (work/'stage.sha256').read_text() != inventory():
        raise RuntimeError('Rust installed inventory changed')
    print('Verified complete Rust sysroot inventory')
    raise SystemExit(0)
if sys.argv[1:]:
    raise RuntimeError('Usage: build.py [--verify]')

cache = work/'cache'
cache.mkdir(exist_ok=True)
temporary = tempfile.TemporaryDirectory(prefix='anova-rust-')
dist = Path(temporary.name)/'dist'
dist.mkdir(exist_ok=True)
for source in config['sources']:
    archive = cache/source['name']
    if not archive.exists():
        part = archive.with_suffix('.part')
        subprocess.run(['curl', '--fail', '--location', '--proto', '=https',
                        '--tlsv1.2', '--retry', '2', '--max-time', '300',
                        source['url'], '-o', str(part)], check=True)
        if digest(part) != source['sha256']:
            raise RuntimeError('Rust input checksum mismatch: '+source['name'])
        part.rename(archive)
    if archive.is_symlink() or not archive.is_file() or digest(archive) != source['sha256']:
        raise RuntimeError('Rust input checksum mismatch: '+source['name'])
    # Validate all members before GNU tar extraction; reject escaping links/devices.
    with tarfile.open(archive) as tar:
        members = []
        for member in tar:
            tarfile.data_filter(member, str(dist))
            if member.isfile() and ('/LICENSE' in member.name or '/COPYRIGHT' in member.name
                    or '/rust-std-' in member.name
                    or member.name.endswith('/bin/rustc') or member.name.endswith('/bin/cargo')
                    or member.name.endswith('/bin/rust-lld')
                    or '/rustc/lib/' in member.name and member.name.endswith('.so')):
                members.append(member)
    subprocess.run(['tar', '--no-same-owner', '-xJf', str(archive), '-C', str(dist), *[m.name for m in members]], check=True)
    for member in members:
        if (dist/member.name).stat().st_size != member.size:
            raise RuntimeError('Incomplete Rust extraction: '+member.name)

final_stage = stage
installation = tempfile.TemporaryDirectory(prefix='stage-', dir=work)
stage = Path(installation.name)/'rs'
triple = config['target']
version = config['version']
compiler = dist/f'rustc-{version}-{triple}'
cargo = dist/f'cargo-{version}-{triple}'
stdlib = dist/f'rust-std-{version}-{triple}'

def copy(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    if destination.stat().st_size != source.stat().st_size:
        raise RuntimeError('Incomplete Rust file copy: '+str(source))
    shutil.copymode(source, destination)

copy(compiler/'rustc/bin/rustc', stage/'bin/rustc')
copy(cargo/'cargo/bin/cargo', stage/'bin/cargo')
subprocess.run(['strip', '--strip-unneeded', '-o', str(stage/'bin/rust-lld'),
                str(compiler/f'rustc/lib/rustlib/{triple}/bin/rust-lld')], check=True)
for p in (compiler/'rustc/lib').glob('*.so'):
    destination = stage/'lib'/p.name
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['strip', '--strip-unneeded', '-o', str(destination), str(p)], check=True)
libraries = stdlib/f'rust-std-{triple}/lib/rustlib/{triple}/lib'
for p in sorted(libraries.rglob('*')):
    if p.is_file():
        copy(p, stage/f'lib/rustlib/{triple}/lib'/p.relative_to(libraries))
# rustc_driver dynamically uses the same musl GCC unwinder as its upstream host build.
bootstrap = root/'build/toolchain/bootstrap.tgz'
bootstrap_lock = json.loads((root/'ports/gcc/inputs.json').read_text())['sources'][0]
if digest(bootstrap) != bootstrap_lock[2]:
    raise RuntimeError('Musl unwinder bootstrap checksum mismatch')
member = 'x86_64-linux-musl-native/lib/libgcc_s.so.1'
subprocess.run(['tar', '--no-same-owner', '-xzf', str(bootstrap), '-C', str(dist), member], check=True)
copy(dist/member, stage/'lib/libgcc_s.so.1')
# Preserve the unwinder's license even in the Rust-only runtime image without /tc.
gcc_lock = next(s for s in json.loads((root/'ports/gcc/inputs.json').read_text())['sources']
                if s[3] == 'gcc-11.2.0')
gcc_archive = root/'build/toolchain'/gcc_lock[0]
if digest(gcc_archive) != gcc_lock[2]:
    raise RuntimeError('Unwinder license source checksum mismatch')
with tarfile.open(gcc_archive) as tar:
    for name in ['COPYING3', 'COPYING.RUNTIME']:
        source = tar.extractfile(gcc_lock[3]+'/'+name)
        destination = stage/'licenses/unwinder'/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(source.read())
for package, directory in [(compiler, 'rustc'), (cargo, 'cargo'), (stdlib, 'std')]:
    for pattern in ['LICENSE*', 'COPYRIGHT*']:
        for p in package.glob(pattern):
            copy(p, stage/'licenses'/directory/p.name)
vendor = config['vendor']
archive = cache/(vendor['name']+'-'+vendor['version']+'.crate')
if not archive.exists():
    subprocess.run(['curl', '--fail', '--location', '--proto', '=https', '--tlsv1.2',
                    '--max-time', '120', vendor['url'], '-o', str(archive)], check=True)
if archive.is_symlink() or digest(archive) != vendor['sha256']:
    raise RuntimeError('Vendored registry archive checksum mismatch')
with tarfile.open(archive) as tar:
    for member in tar:
        tarfile.data_filter(member, str(dist))
subprocess.run(['tar', '--no-same-owner', '-xf', str(archive), '-C', str(dist)], check=True)
vendored = stage/'vendor'/vendor['name']
shutil.copytree(dist/(vendor['name']+'-'+vendor['version']), vendored)
files = {str(p.relative_to(vendored)): digest(p) for p in sorted(vendored.rglob('*')) if p.is_file()}
(vendored/'.cargo-checksum.json').write_text(json.dumps({'files': files, 'package': vendor['sha256']})+'\n')
copy(root/'ports/rust/inputs.json', stage/'licenses/inputs.json')
copy(root/'docs/third-party/m12.md', stage/'licenses/PROFILE.md')
final_stage.parent.mkdir(parents=True, exist_ok=True)
previous = Path(installation.name)/'previous'
if final_stage.exists():
    final_stage.rename(previous)
try:
    stage.rename(final_stage)
except OSError:
    if previous.exists():
        previous.rename(final_stage)
    raise
stage = final_stage
(work/'stage.sha256').write_text(inventory())
stamp.write_text(recipe)
print('Prepared locked upstream Rust', version, 'musl host tools and std at', stage)
