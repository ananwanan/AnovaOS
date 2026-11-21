#!/bootstrap/sh
# SPDX-License-Identifier: Apache-2.0
# Build the upstream LLVMDemangle library serially, then check failure retention and reboot reuse.

set -u
PATH=/bootstrap:/tc/bin:/linux
LC_ALL=C
SOURCE_DATE_EPOCH=1758153600
src=/m18-llvm
work=/persist/llvm-demangle
TMPDIR=$work/tmp
export PATH LC_ALL SOURCE_DATE_EPOCH TMPDIR
# The fixed flags are deliberately word-split before the separate path and file arguments.
cxxflags='--sysroot=/tc -std=c++17 -O2 -pipe -static -no-pie -fno-pie -fcf-protection=none
-fno-exceptions -fno-rtti -Wl,--build-id=none --param=ggc-min-expand=100 --param=ggc-min-heapsize=32768'

fail() {
    printf '[M18:LLVM:FAIL] %s\n' "$1" >&2
    exit 1
}

cxx() {
    /tc/bin/g++ $cxxflags -I"$src/include" "$@"
}

compile() {
    cxx -MMD -c "$src/lib/Demangle/$1.cpp" -o "$1.o"
}

case "${1:-}" in
    build)
        mkdir "$work" && mkdir "$TMPDIR" && cd "$work" || fail 'fresh workspace'
        printf 'LLVM build workspace data\n' > retained.txt || fail 'write initial data'
        /tc/bin/g++ $cxxflags -I"$src/include" -MMD -c \
            "$src/lib/Demangle/ItaniumDemangle.cpp" -o ItaniumDemangle.o &
        compiler=$!
        printf '[M18:LLVM:ACTIVE] translation-unit=ItaniumDemangle\n'
        if ! IFS= read -r request || test "$request" != ping \
            || ! kill -0 "$compiler" || test -s ItaniumDemangle.o; then
            wait "$compiler"
            fail 'UART response while the compiler is live'
        fi
        printf '[M18:LLVM:PONG] compiler-live=true\n'
        wait "$compiler" || fail 'ItaniumDemangle compile'
        for unit in DLangDemangle Demangle MicrosoftDemangle MicrosoftDemangleNodes RustDemangle; do
            compile "$unit" || fail "$unit compile"
        done
        /tc/bin/ar rcsD libLLVMDemangle.a DLangDemangle.o Demangle.o ItaniumDemangle.o \
            MicrosoftDemangle.o MicrosoftDemangleNodes.o RustDemangle.o || fail 'archive'
        cxx -s "$src/check.cpp" libLLVMDemangle.a -o demangle || fail 'link check'
        ./demangle || fail 'demangler execution'
        printf '[M18:LLVM:BUILT] objects=6\n'
        ;;
    recover)
        cd "$work" || fail 'missing retained workspace'
        if cxx -c "$src/bad.cpp" -o rejected.o > syntax.log 2>&1; then
            fail 'invalid source compiled'
        else
            test "$?" -eq 1 && test ! -e rejected.o || fail 'unexpected syntax rejection'
        fi
        if cxx -s "$src/check.cpp" -o rejected.elf > link.log 2>&1; then
            fail 'linked without LLVMDemangle'
        else
            test "$?" -eq 1 && test ! -e rejected.elf || fail 'unexpected link rejection'
        fi
        cxx -s "$src/check.cpp" libLLVMDemangle.a -o recovered || fail 'link retained archive'
        cmp -s recovered demangle || fail 'retained archive linked a different executable'
        ./recovered && ./demangle || fail 'retained artifact execution'
        printf '[M18:LLVM:RECOVERY] syntax=1 link=1 archive-linked=true prior-executed=true\n'
        ;;
    *)
        fail 'expected build or recover'
        ;;
esac
