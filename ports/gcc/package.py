# SPDX-License-Identifier: Apache-2.0
# Assemble only the locked offline C/C++ profile and record every installed byte.
from pathlib import Path
import hashlib
import shutil
import sys

root = Path(__file__).resolve().parents[2]
work = root / 'build/toolchain'
bootstrap = work / 'x86_64-linux-musl-native'
stage = work / 'stage/tc'
out = work / 'package/tc'
if out.exists():
    shutil.rmtree(out)
version = '11.2.0'
triple = 'x86_64-linux-musl'

def copy(source, destination):
    source = source.resolve(strict=True)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    shutil.copymode(source, destination)

def tree(source, destination):
    for p in sorted(source.rglob('*')):
        if p.is_file():
            copy(p, destination / p.relative_to(source))

for name in ['gcc', 'g++', 'as', 'ld', 'ar', 'ranlib', 'make']:
    copy(stage / 'bin' / name, out / 'bin' / name)
for name in ['cc1', 'cc1plus', 'collect2']:
    copy(stage / f'libexec/gcc/{triple}/{version}/{name}',
         out / f'libexec/gcc/{triple}/{version}/{name}')
tree(stage / f'lib/gcc/{triple}/{version}/include',
     out / f'lib/gcc/{triple}/{version}/include')
# Install only source-built GCC 11.2.0 and musl 1.2.4 target libraries.
for name in ['crtbegin.o', 'crtbeginS.o', 'crtbeginT.o', 'crtend.o', 'crtendS.o',
             'libgcc.a']:
    copy(stage / f'lib/gcc/{triple}/{version}/{name}',
         out / f'lib/gcc/{triple}/{version}/{name}')
for name in ['crt1.o', 'Scrt1.o', 'crti.o', 'crtn.o', 'libc.a', 'libm.a',
             'libpthread.a', 'libdl.a', 'librt.a', 'libstdc++.a', 'libsupc++.a']:
    copy(stage / 'lib' / name, out / 'lib' / name)
for p in sorted((stage / 'include').iterdir()):
    if p.is_file():
        copy(p, out / 'include' / p.name)
    elif p.name in ['bits', 'sys', 'arpa', 'net', 'netinet', 'netpacket']:
        tree(p, out / 'include' / p.name)
tree(stage / f'include/c++/{version}', out / f'include/c++/{version}')
for name in ['as', 'ld', 'ar', 'ranlib']:
    copy(stage / 'bin' / name, out / triple / 'bin' / name)
for source, name in [
    (work/'gcc-11.2.0/COPYING3', 'gcc-COPYING3'),
    (work/'gcc-11.2.0/COPYING.RUNTIME', 'gcc-COPYING.RUNTIME'),
    (work/'binutils-2.37/COPYING3', 'binutils-COPYING3'),
    (work/'make-4.4.1/COPYING', 'make-COPYING'),
    (root/'docs/third-party/m11.md', 'PROFILE.md'),
    (work/'musl-1.2.4/COPYRIGHT', 'musl-COPYRIGHT'),
    (work/'gmp-6.2.1/COPYING.LESSERv3', 'gmp-COPYING.LESSERv3'),
    (work/'mpfr-4.1.0/COPYING.LESSER', 'mpfr-COPYING.LESSER'),
    (work/'mpc-1.2.1/COPYING.LESSER', 'mpc-COPYING.LESSER'),
]:
    copy(source, out / 'licenses' / name)
manifest = []
for p in sorted(out.rglob('*')):
    if p.is_file():
        path = '/tc/' + str(p.relative_to(out))
        if len(path.encode()) > 128:
            sys.exit('Toolchain path exceeds guest limit: ' + path)
        manifest.append(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + path)
(work/'package.sha256').write_text('\n'.join(manifest) + '\n')
print('Packed', len(manifest), 'locked toolchain files')
