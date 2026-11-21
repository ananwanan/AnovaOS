#!/bootstrap/sh
# SPDX-License-Identifier: Apache-2.0
# Exercise applet output, exit status and file effects on a fresh M18 workspace.

PATH=/bootstrap
LC_ALL=C
export PATH LC_ALL
exec 4>&2

fail() {
    printf '[M18:POSIX:FAIL] %s\n' "$*" >&4
    exit 1
}

expect_status() {
    expected=$1
    shift
    "$@"
    actual=$?
    test "$actual" -eq "$expected" || fail "$1: exit $actual, expected $expected"
}

check_content() {
    actual=$(cat "$1") || fail "cannot read $1"
    test "$actual" = "$2" || fail "unexpected contents: $1"
}

test "$#" -eq 1 || fail 'one new work directory is required'
expect_status 0 mkdir "$1"
cd "$1" || fail 'cannot enter the work directory'
expect_status 0 mkdir -p ./
expect_status 0 mkdir -p created/
test -d created || fail 'trailing slash changed mkdir target'
expect_status 0 rmdir created
printf '[M18:POSIX:TOOL] name=mkdir dot=ok trailing-slash=ok\n'

test "$(type echo)" = 'echo is a shell builtin' || fail 'missing ash echo builtin'
test "$(type printf)" = 'printf is a shell builtin' || fail 'missing ash printf builtin'
payload=abcdefghijklmnopqrstuvwxyz
for iteration in 1 2 3 4 5 6 7 8 9; do
    payload=$payload$payload
done
test "${#payload}" -eq 13312 || fail 'bad long printf fixture'
test "$(printf '%s' "$payload")" = "$payload" || fail 'ash printf truncated its argument'
printf '[M18:POSIX:SHELL] long-printf=13312 echo=builtin\n'

# Ash puts here-documents up to PIPE_BUF into a pipe before starting the reader.
body=0123456789abcdef
for bit in 1 2 3 4 5 6 7; do
    body=$body$body
done
test "${#body}" -eq 2048 || fail 'unexpected here-document length'
printf '%s\n' "$body" >heredoc.expected || fail 'cannot write the here-document reference'
expect_status 0 cat - >heredoc.out <<M18_HEREDOC
$body
M18_HEREDOC
expect_status 0 cmp -s heredoc.expected heredoc.out
printf '[M18:POSIX:SHELL] heredoc=2049\n'

printf 'beta:2\nalpha:7\nbeta:4\n' > input || fail 'cannot create input'
expect_status 0 sed -n 's/^beta:/x:/p' input > sed.out
check_content sed.out 'x:2
x:4'
expect_status 1 sed 's/[/' input > sed-error.out 2> sed-error.log
test -s sed-error.log || fail 'sed omitted its malformed expression diagnostic'
printf '[M18:POSIX:TOOL] name=sed output=ok malformed=1\n'

expect_status 0 awk -F: '$1 == "beta" { sum += $2 } END { print sum }' input > awk.out
check_content awk.out 6
expect_status 1 awk 'BEGIN {' > awk-error.out 2> awk-error.log
test -s awk-error.log || fail 'awk omitted its malformed program diagnostic'
printf '[M18:POSIX:TOOL] name=awk output=ok malformed=1\n'

expect_status 0 grep -n '^alpha:' input > grep.out
check_content grep.out '2:alpha:7'
expect_status 1 grep '^gamma:' input > grep-none.out
test ! -s grep-none.out || fail 'grep emitted a nonexistent match'
expect_status 2 grep beta missing > grep-error.out 2> grep-error.log
test -s grep-error.log || fail 'grep omitted its missing input diagnostic'
printf '[M18:POSIX:TOOL] name=grep output=ok no-match=1 missing=2\n'

# The upstream non-GNU grep probe uses diff to verify its long-line output.
printf '%sGREP\n' "$payload" >grep-long.in || fail 'cannot write long grep input'
expect_status 0 grep -e 'GREP$' -e '-(cannot match)-' <grep-long.in >grep-long.out
expect_status 0 diff grep-long.in grep-long.out
expect_status 1 diff input grep-long.out >diff.out
expect_status 0 find . \( -type f -name 'grep-long.out' \) -exec cat '{}' ';' >find.out
expect_status 0 cmp -s grep-long.out find.out
printf '[M18:POSIX:TOOL] name=grep-diff-find long-line=13316 match=0 different=1\n'

expect_status 0 expr 6 + 7 > expr.out
check_content expr.out 13
expect_status 1 expr 6 - 6 > expr-zero.out
check_content expr-zero.out 0
expect_status 2 expr 6 + > expr-error.out 2> expr-error.log
test -s expr-error.log || fail 'expr omitted its malformed expression diagnostic'
printf '[M18:POSIX:TOOL] name=expr output=ok zero=1 malformed=2\n'

expect_status 0 tr 'a-z' 'A-Z' < input > tr.out
check_content tr.out 'BETA:2
ALPHA:7
BETA:4'
expect_status 1 tr -Z < input > tr-error.out 2> tr-error.log
test -s tr-error.log || fail 'tr omitted its invalid option diagnostic'
printf '[M18:POSIX:TOOL] name=tr output=ok invalid-option=1\n'

expect_status 0 sort input > sort.out
check_content sort.out 'alpha:7
beta:2
beta:4'
expect_status 2 sort missing > sort-error.out 2> sort-error.log
test -s sort-error.log || fail 'sort omitted its missing input diagnostic'
printf '[M18:POSIX:TOOL] name=sort output=ok missing=2\n'

printf 'beta:2\nalpha:7\nbeta:4\n' > copied || fail 'cannot create compare operand'
expect_status 0 cmp -s input copied
expect_status 1 cmp -s input sort.out
expect_status 2 cmp input missing > cmp-error.out 2> cmp-error.log
test -s cmp-error.log || fail 'cmp omitted its missing operand diagnostic'
printf '[M18:POSIX:TOOL] name=cmp same=0 different=1 missing=2\n'

printf 'old\n' > renamed || fail 'cannot create rename destination'
exec 3< renamed
expect_status 0 mv copied renamed
test ! -e copied || fail 'mv left its original pathname'
expect_status 0 cmp -s input renamed
expect_status 0 cat <&3 > mv-old.out
exec 3<&-
check_content mv-old.out old
expect_status 1 mv missing renamed > mv-error.out 2> mv-error.log
test -s mv-error.log || fail 'mv omitted its missing source diagnostic'
expect_status 0 cmp -s input renamed
printf '[M18:POSIX:TOOL] name=mv replacement=ok old-fd=ok missing=1\n'

expect_status 0 install -m 755 input installed
test -x installed || fail 'install lost the requested execute mode'
expect_status 0 cmp -s input installed
expect_status 1 install missing installed > install-error.out 2> install-error.log
test -s install-error.log || fail 'install omitted its missing source diagnostic'
expect_status 0 cmp -s input installed
printf '[M18:POSIX:TOOL] name=install mode=755 bytes=ok missing=1\n'

# An archive-provided symbolic link resolves, while the bounded guest creation
# interface currently rejects both runtime link syscalls. Check the real errors.
expect_status 0 cmp -s /bootstrap/sed /bootstrap/busybox
expect_status 1 ln missing rejected-missing > ln-missing.out 2> ln-missing.log
expect_status 0 grep -q 'No such file or directory' ln-missing.log
expect_status 1 ln input rejected-hard > ln-hard.out 2> ln-hard.log
expect_status 0 grep -q 'Function not implemented' ln-hard.log
expect_status 1 ln -s input rejected-symbolic > ln-symbolic.out 2> ln-symbolic.log
expect_status 0 grep -q 'Function not implemented' ln-symbolic.log
for path in rejected-missing rejected-hard rejected-symbolic; do
    test ! -e "$path" && test ! -L "$path" || fail "ln left $path"
done
expect_status 0 cmp -s input renamed
printf '[M18:POSIX:TOOL] name=ln existing-symlink=ok missing=1 hard=enosys symbolic=enosys\n'

printf '[M18:POSIX] success\n'
