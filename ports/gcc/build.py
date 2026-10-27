# SPDX-License-Identifier: Apache-2.0
# Cross-build the guest tools, CRT, libc and C++ runtime from checksum-locked inputs.
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
from fetch import fetch

root = Path(__file__).resolve().parents[2]
work = root / 'build/toolchain'
work.mkdir(parents=True, exist_ok=True)
lock = json.loads((root/'ports/gcc/inputs.json').read_text())
inputs = lock['sources']
recipe = hashlib.sha256()
for name in ['build.py', 'build.sh', 'inputs.json', 'fetch.py']:
    recipe.update((root/'ports/gcc'/name).read_bytes())
stamp = work/'source-build.sha256'

def stage_manifest():
    result = []
    base = work/'stage'
    for p in sorted(base.rglob('*')):
        if p.is_file():
            if not p.resolve().is_relative_to(base.resolve()):
                raise RuntimeError('Installed toolchain link escapes its stage: '+str(p))
            result.append(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(base)))
    return '\n'.join(result)+'\n'

if sys.argv[1:] == ['--verify']:
    if not stamp.exists() or stamp.read_text().strip() != recipe.hexdigest():
        raise RuntimeError('Source build recipe changed; run cargo xtask toolchain-build')
    expected = (work/'stage.sha256').read_text()
    if expected != stage_manifest():
        raise RuntimeError('Installed source build changed; rebuild before packaging')
    print('Verified complete installed source-build inventory', flush=True)
    raise SystemExit(0)

for name, url, expected, directory in inputs:
    archive = work/name
    fetch(archive, [url, *lock.get('mirrors', {}).get(name, [])], expected,
          os.environ.get('ANOVA_SOURCE_CACHE'))
    if not archive.is_file() or archive.is_symlink() or hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise RuntimeError('Input checksum mismatch: '+name)
    if not (work/directory).exists():
        with tarfile.open(archive) as source:
            def safe_member(member, destination):
                # The locked bootstrap has one guest-absolute loader link. Relocate
                # that exact link into its sysroot; retain data_filter for all paths.
                if (name == 'bootstrap.tgz' and member.issym()
                        and member.name == directory+'/lib/ld-musl-x86_64.so.1'
                        and member.linkname == '/lib/libc.so'):
                    member = member.replace(linkname='libc.so')
                return tarfile.data_filter(member, destination)
            source.extractall(work, filter=safe_member)

if stamp.exists() and stamp.read_text().strip() == recipe.hexdigest():
    if (work/'stage.sha256').read_text() != stage_manifest():
        raise RuntimeError('Cached toolchain contents changed')
    print('Reusing verified completed source build', flush=True)
    raise SystemExit(0)

area = work/'rebuild'
area.mkdir(exist_ok=True)
stage = area/'stage'
stage.mkdir(exist_ok=True)
deps = area/'deps'
bootstrap = work/'x86_64-linux-musl-native'
host = area/'host-tools'
host.mkdir(exist_ok=True)
for name in ['ar', 'as', 'ld', 'nm', 'objcopy', 'objdump', 'ranlib', 'readelf', 'strip', 'gcc', 'g++', 'c++', 'cpp']:
    path = host/('x86_64-linux-musl-'+name)
    if not path.exists():
        path.symlink_to(bootstrap/'bin'/name)
env = os.environ.copy()
env.update(CC=str(bootstrap/'bin/gcc'), CXX=str(bootstrap/'bin/g++'),
           CFLAGS='-O2 -g0 -fcf-protection=none', CXXFLAGS='-O2 -g0 -fcf-protection=none',
           LDFLAGS='-static -s', PATH=str(Path(shutil.which('gcc')).parent)+':'+str(host)+':'+env['PATH'])
jobs = min(8, max(1, int(env.get('ANOVA_BUILD_JOBS', '6'))))
common = ['--build=x86_64-pc-linux-gnu', '--host=x86_64-linux-musl']

def run(args, cwd, log, settings=None):
    print('Building', cwd.name, ':', ' '.join(map(str,args[:3])), flush=True)
    with log.open('ab') as output:
        subprocess.run(list(map(str,args)), cwd=cwd, env=settings or env,
                       stdout=output, stderr=subprocess.STDOUT, check=True)

def project(name, source, options, settings=None, targets=None, flags=None):
    cwd = area/name
    cwd.mkdir(exist_ok=True)
    log = area/(name+'.log')
    run([work/source/'configure', *options], cwd, log, settings)
    run(['make', '-j'+str(jobs), *(targets or []), *(flags or [])], cwd, log, settings)
    return cwd, log

for name, source, options in [
    ('gmp', 'gmp-6.2.1', ['--disable-assembly']),
    ('mpfr', 'mpfr-4.1.0', ['--with-gmp='+str(deps)]),
    ('mpc', 'mpc-1.2.1', ['--with-gmp='+str(deps), '--with-mpfr='+str(deps)]),
]:
    cwd, log = project(name, source, common+['--prefix='+str(deps), '--disable-shared']+options)
    run(['make', 'install'], cwd, log)

cwd, log = project('musl', 'musl-1.2.4', ['--prefix=/tc', '--syslibdir=/tc/lib', '--disable-shared'])
run(['make', 'install', 'DESTDIR='+str(stage)], cwd, log)
usr = stage/'tc/usr'
if not usr.exists(): usr.symlink_to('.')

cwd, log = project('binutils', 'binutils-2.37', common+[
    '--target=x86_64-linux-musl', '--prefix=/tc', '--with-sysroot=/tc', '--disable-nls',
    '--disable-shared', '--disable-werror', '--disable-gdb', '--disable-gprof', '--disable-gold',
    '--disable-sim', '--disable-libctf', '--without-zstd', '--without-zlib', '--enable-deterministic-archives'],
    targets=['all-binutils', 'all-gas', 'all-ld'])
for name in ['gas/as-new', 'ld/ld-new', 'binutils/ar', 'binutils/ranlib']:
    (cwd/name).unlink(missing_ok=True)
run(['make', '-j'+str(jobs), 'all-binutils', 'all-gas', 'all-ld', 'LDFLAGS=-all-static -s'], cwd, log)
run(['make', 'install-binutils', 'install-gas', 'install-ld', 'DESTDIR='+str(stage)], cwd, log)

make_env = env.copy()
make_env.update(ac_cv_func_posix_spawn='no', ac_cv_func_vfork='no', ac_cv_func_vfork_works='no')
cwd, log = project('make', 'make-4.4.1', common+['--prefix=/tc', '--disable-nls', '--without-guile'], settings=make_env)
run(['make', 'install', 'DESTDIR='+str(stage)], cwd, log, make_env)

cwd, log = project('gcc', 'gcc-11.2.0', common+[
    '--target=x86_64-linux-musl', '--prefix=/tc', '--with-sysroot=/tc',
    '--with-build-sysroot='+str(stage/'tc'), '--with-gmp='+str(deps), '--with-mpfr='+str(deps),
    '--with-mpc='+str(deps), '--enable-languages=c,c++', '--disable-bootstrap', '--disable-multilib',
    '--disable-nls', '--disable-shared', '--disable-libsanitizer', '--disable-libquadmath',
    '--disable-libgomp', '--disable-libssp', '--disable-libstdcxx-pch', '--disable-libvtv',
    '--disable-libitm', '--disable-lto', '--disable-cet', '--without-isl', '--disable-werror'],
    targets=['all-gcc'], flags=['CC_FOR_BUILD='+shutil.which('gcc'), 'CXX_FOR_BUILD='+shutil.which('g++')])
run(['make', 'install-gcc', 'DESTDIR='+str(stage)], cwd, log)
# The Canadian cross build uses the locked musl bootstrap for target runtime objects.
run(['make', '-j'+str(jobs), 'all-target-libgcc', 'all-target-libstdc++-v3',
     'CXXFLAGS_FOR_TARGET=-O2 -g0 -nostdinc++'], cwd, log)
run(['make', 'install-target-libgcc', 'install-target-libstdc++-v3', 'DESTDIR='+str(stage)], cwd, log)

# Publish a complete new copy before retiring this recipe's previous generated tree.
final = work/'stage'
with tempfile.TemporaryDirectory(prefix='install-', dir=work) as directory:
    candidate = Path(directory)/'stage'
    previous = Path(directory)/'previous'
    shutil.copytree(stage, candidate, symlinks=True)
    if final.exists():
        final.rename(previous)
    try:
        candidate.rename(final)
    except OSError:
        if previous.exists():
            previous.rename(final)
        raise
(work/'stage.sha256').write_text(stage_manifest())
stamp.write_text(recipe.hexdigest()+'\n')
print('Built source toolchain in', final, flush=True)
