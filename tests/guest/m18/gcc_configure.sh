#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Exercise seed dependencies and build the selected original GCC targets in the guest.
set -eu
PATH=/bootstrap:/bin:/tc/bin:/linux
LC_ALL=C
CONFIG_SHELL=/bootstrap/sh
SHELL=/bootstrap/sh
CONFIG_SITE=/dev/null
CC=/bin/gcc
CXX=/bin/g++
TMPDIR=/tmp
export PATH LC_ALL CONFIG_SHELL SHELL CONFIG_SITE CC CXX TMPDIR

fail() {
    printf '[M18:GCC:FAIL] phase=%s\n' "$1" >&2
    exit 1
}

profile=${1:-entry}
case "$profile" in
    entry) source=/m18-source ;;
    stage1) source=/persist/m18-source ;;
    *) fail unknown-profile ;;
esac

mkdir /persist/m18 || fail create-workspace
cd /persist/m18 || fail enter-workspace
/bootstrap/sh /m18-gcc/posix.sh /persist/m18/posix || fail posix
if test "$profile" = stage1; then
    /m18-gcc/exec-bounds || fail exec-bounds
    printf 'm18-script\n' > move-if-change.tmp || fail script-probe-input
    "$source/move-if-change" move-if-change.tmp move-if-change.out || fail script-exec
    test ! -e move-if-change.tmp && test "$(cat move-if-change.out)" = m18-script \
        || fail script-probe-output
    printf '[M18:GCC:SCRIPT] move-if-change=0\n'
    for options in -Lt -t; do
        ordered=$(ls "$options" "$source/zlib/configure" move-if-change.out) || fail timestamp-list
        test "$ordered" = "move-if-change.out
$source/zlib/configure" || fail timestamp-order
    done
    printf '[M18:GCC:TOOLS] ls-timestamp-order=ok\n'
fi
cp /m18-gcc/arithmetic.c .
if gcc -c arithmetic.c -o rejected.o >missing-header.log 2>&1; then
    fail missing-header-accepted
fi
test ! -e rejected.o || fail rejected-object
if gcc -O2 -static -I/m18-arithmetic/include arithmetic.c \
    -o /tmp/m18-rejected >missing-library.log 2>&1; then
    fail missing-library-accepted
fi
test ! -e /tmp/m18-rejected || fail rejected-executable
gcc -O2 -static -I/m18-arithmetic/include arithmetic.c \
    -L/m18-arithmetic/lib -lmpc -lmpfr -lgmp -lm -o /tmp/m18-arithmetic
cp /tmp/m18-arithmetic arithmetic
chmod 755 arithmetic
./arithmetic

if test "$profile" = stage1; then
    mkdir tmp
    TMPDIR=/persist/m18/tmp
    CC=/tc/bin/gcc
    CXX=/tc/bin/g++
    CFLAGS='-O1 -g0 -pipe -fcf-protection=none'
    CXXFLAGS="$CFLAGS"
    LDFLAGS='-static -s'
    export CFLAGS CXXFLAGS LDFLAGS
fi

mkdir configured
cd configured
/bootstrap/sh "$source/configure" \
    --srcdir="$source" --cache-file=/dev/null \
    --build=x86_64-linux-musl --host=x86_64-linux-musl --target=x86_64-linux-musl \
    --prefix=/persist/m18/gcc-stage1 --with-sysroot=/tc \
    --with-gmp=/m18-arithmetic --with-mpfr=/m18-arithmetic --with-mpc=/m18-arithmetic \
    --enable-languages=c,c++ --disable-bootstrap --disable-multilib \
    --disable-nls --disable-shared --disable-lto --without-isl \
    </dev/null > configure.stdout 2>&1 &
configure_pid=$!
printf '[M18:GCC:UART] waiting\n'
IFS= read -r request || fail serial-read
test "$request" = probe || fail serial-input
kill -0 "$configure_pid" || fail configure-not-live
printf '[M18:GCC:UART] configure-live\n'
if wait "$configure_pid"; then
    configure_status=0
else
    configure_status=$?
fi
printf '%s\n' "$configure_status" >configure.exit
printf '[M18:GCC:CONFIGURE] exit=%s\n' "$configure_status"
if test "$configure_status" -eq 0; then
    if /tc/bin/make -j1 SHELL=/bootstrap/sh all-gcc > make.stdout 2>&1; then
        make_status=0
    else
        make_status=$?
    fi
    printf '%s\n' "$make_status" >make.exit
    printf '[M18:GCC:MAKE] exit=%s\n' "$make_status"
    if test "$profile" = stage1 && test "$make_status" -eq 0; then
        if /bootstrap/sh /m18-gcc/gcc_stage1.sh build; then
            stage_status=0
        else
            stage_status=$?
        fi
        printf '%s\n' "$stage_status" >stage1.exit
        printf '[M18:GCC:STAGE1-CHECK] exit=%s\n' "$stage_status"
    elif test "$profile" = entry && grep -q 'cannot find sources (tree.c)' make.stdout; then
        # Reach the unmodified recursive recipe separately while the source subset lacks tree.c.
        if /tc/bin/make -j1 -o configure-gcc SHELL=/bootstrap/sh all-gcc \
            >recursive-probe.stdout 2>&1; then
            recursive_status=0
        else
            recursive_status=$?
        fi
        printf '%s\n' "$recursive_status" >recursive-probe.exit
        printf '[M18:GCC:RECURSIVE-PROBE] exit=%s\n' "$recursive_status"
    fi
fi
cd ..
./arithmetic
