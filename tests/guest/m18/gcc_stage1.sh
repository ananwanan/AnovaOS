#!/bootstrap/sh
# SPDX-License-Identifier: Apache-2.0
# Exercise the guest-built GCC drivers, frontends and retained C/C++ executables.

PATH=/bootstrap:/tc/bin:/linux
LC_ALL=C
SOURCE_DATE_EPOCH=1627421400
compiler=/persist/m18/configured/gcc
tools=/m18-gcc/tools
work=/persist/m18/stage1-check
TMPDIR=$work/tmp
export PATH LC_ALL SOURCE_DATE_EPOCH TMPDIR
unset GCC_EXEC_PREFIX COMPILER_PATH LIBRARY_PATH

fail() {
    printf '[M18:GCC:STAGE1:FAIL] %s\n' "$1" >&2
    exit 1
}

stage_cc() {
    "$compiler/xgcc" -B"$tools/" -B"$compiler/" -B/tc/bin/ -B/tc/lib/gcc/x86_64-linux-musl/11.2.0/ \
        -B/tc/lib/ --sysroot=/tc -static -no-pie -fno-pie -fcf-protection=none \
        -fno-use-linker-plugin -Wl,--build-id=none "$@"
}

stage_cxx() {
    "$compiler/xg++" -B"$tools/" -B"$compiler/" -B/tc/bin/ -B/tc/lib/gcc/x86_64-linux-musl/11.2.0/ \
        -B/tc/lib/ --sysroot=/tc -static -no-pie -fno-pie -fcf-protection=none \
        -fno-use-linker-plugin -Wl,--build-id=none -isystem /tc/include/c++/11.2.0 \
        -isystem /tc/include/c++/11.2.0/x86_64-linux-musl \
        -isystem /tc/include/c++/11.2.0/backward "$@"
}

check_frontends() {
    cc1=$(stage_cc -print-prog-name=cc1) || return 1
    cc1plus=$(stage_cxx -print-prog-name=cc1plus) || return 1
    cc_version=$(stage_cc -dumpfullversion) || return 1
    cxx_version=$(stage_cxx -dumpfullversion) || return 1
    assembler=$(stage_cc -print-prog-name=as) || return 1
    printf 'cc1=%s\ncc1plus=%s\ngcc=%s\ng++=%s\n' "$cc1" "$cc1plus" "$cc_version" "$cxx_version"
    printf 'as=%s\n' "$assembler"
    test "$cc1" = "$compiler/cc1" && test "$cc1plus" = "$compiler/cc1plus" \
        && test "$cc_version" = 11.2.0 && test "$cxx_version" = 11.2.0 \
        && test "$assembler" = "$tools/as"
}

execute() {
    status=0
    "./$1.elf" || status=$?
    test "$status" -eq "$2"
}

case "${1:-}" in
    build)
        mkdir "$work" && mkdir "$TMPDIR" && cd "$work" || fail 'create check workspace'
        trap 'status=$?; printf "%s\n" "$status" > exit' EXIT
        check_frontends > driver.log 2>&1 || fail 'resolve guest-built cc1 and cc1plus'
        stage_cc -v -O2 -pipe -c /m18-gcc/stage1.c -o c.o > c-compile.log 2>&1 \
            || fail 'C compilation'
        stage_cc -v -s -Wl,-debug c.o -o c.elf > c-link.log 2>&1 || fail 'C link'
        stage_cxx -v -O2 -pipe -std=c++17 -c /m18-gcc/stage1.cpp -o cpp.o > cpp-compile.log 2>&1 \
            || fail 'C++ compilation'
        stage_cxx -v -s -Wl,-debug cpp.o -o cpp.elf > cpp-link.log 2>&1 || fail 'C++ link'
        printf '#error m18-stage1-syntax\n' > bad.c || fail 'write syntax fixture'
        if stage_cc -c bad.c -o rejected.o > syntax.log 2>&1; then
            fail 'accepted invalid source'
        else
            test "$?" -eq 1 && test ! -e rejected.o || fail 'syntax failure status/output'
        fi
        # Keep C linkage so the expected unresolved symbol is stable across diagnostics.
        printf '%s\n' 'extern "C" int m18_stage1_missing();' \
            'int main() { return m18_stage1_missing(); }' > badlink.cpp \
            || fail 'write unresolved C++ reference'
        if stage_cxx -pipe badlink.cpp -o rejected.elf > link.log 2>&1; then
            fail 'linked unresolved symbol'
        else
            test "$?" -eq 1 && test ! -e rejected.elf || fail 'link failure status/output'
        fi
        execute c 17 > c.out && test "$(cat c.out)" = 'stage1-c=42' || fail 'C execution'
        execute cpp 23 > cpp.out && test "$(cat cpp.out)" = 'stage1-cpp=42' || fail 'C++ execution'
        printf '[M18:GCC:STAGE1] c=17 cpp=23 syntax=1 link=1\n'
        ;;
    recover)
        cd "$work" && check_frontends || fail 'recover compiler frontends'
        c_output=$(execute c 17) && test "$c_output" = 'stage1-c=42' || fail 'recover C executable'
        cpp_output=$(execute cpp 23) && test "$cpp_output" = 'stage1-cpp=42' || fail 'recover C++ executable'
        printf '[M18:GCC:RECOVERY] c=17 cpp=23 frontends=verified\n'
        ;;
    *)
        fail 'expected build or recover'
        ;;
esac
