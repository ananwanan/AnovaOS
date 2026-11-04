<!-- Specify controlled compiler identity, ownership, log bounds, publication and retention for M17.4 batch one. -->
# M17.4 batch one: build diagnostics and task status

**English** | [简体中文](../../zh-CN/milestones/M17_BUILD.md)

This is the first bounded delivery under [M17.4](M17_DESKTOP.md), not the whole
resource/task/service milestone. Executed checks belong in [M17.4 validation](../validation/M17.4.md).

Focus the ordinary launcher and press `b`. `g` explicitly starts the fixed command:

```sh
/bin/gcc -fdiagnostics-color=never /persist/build.c -o /persist/build-app.next
```

Create/save `/persist/build.c` with Files or the existing PTY. Use the admitted M11
compiler profile and a Linux-personality work disk for runnable outputs:

```sh
cargo xtask terminal-image --toolchain
cargo xtask ext4-journal-image --linux build/disks/build-work.ext4
cargo xtask run --desktop --toolchain --disk build/disks/build-work.ext4
cargo xtask desktop-build-test
```

The shell-only desktop reports an actual start failure when the compiler is absent.
`[`/`]` selects the previous/next retained attempt, PgUp/PgDn moves ten log lines,
and Esc returns to the launcher. Existing `f`/`t`, Files unsaved protection, COM1
recovery and the shared three-replacement limit remain. Diagnostics uses the
launcher's existing window; no GUI or kernel resource quota is raised.

## Identity, owner and lifecycle

Each explicit start attempt receives a launcher-local monotonic attempt number,
including a failed start. Successful START also returns the never-reused compiler
leader PID within the current kernel boot, which remains the exact job identity across exec. A scheduler slot
number is never an identity. Failed starts have no job PID and cannot inherit a
previous result. The exact native creator owns the private master and handle;
forked native processes, peers and the interactive PTY interface cannot use it.

The kernel directly admits `/bin/gcc` as `linux.x86_64.v1` and passes fixed argv.
There is no interactive shell command, success-marker parser or arbitrary command
input. The existing immutable executable admission and work-disk ABI marker apply.
The leader receives only slave descriptors 0/1/2, its own session/process group,
the existing bounded compiler environment and creator cwd. Linux descendants keep
the existing PTY-family tag across fork/exec/setsid. They acquire no GUI authority.

One controlled job is serialized globally until CLOSE. It shares the existing
four PTY slots with interactive terminals. START exclusively reserves the staging
file; an existing staging/recovery file is rejected without truncation. Partial
START failure closes descriptors, terminates/reaps any created child and removes
only the newly reserved staging path. Filesystem I/O errors can leave staging
evidence requiring inspection; resource rollback does not promise storage repair.
Other writers must coordinate access to these fixed paths; this is not a file lock
or a filesystem sandbox.

STATUS copies the leader's real native `WaitStatus` without reaping it. It does
not report the interactive shell's status or derive state from output. On leader
exit the UI drains buffered output in bounded turns. If a surviving descendant
keeps the slave open, the UI stops draining at a temporarily empty queue or after
at most three UI turns with a delivered leader status. It records `cut=true`
without EOF, then CLOSE terminates/reaps the tagged family, including a continuous writer. It does not wait forever
for detached writers or promise their future output. READ/STATUS copy failures
retain undelivered bytes/status in kernel state. The UI retries status delivery;
unknown status never becomes a successful exit.

Only an actual reason-0/code-0 result with complete, error-free capture requests the existing
checked file replacement from staging to `/persist/build-app`. Nonzero/abnormal
builds leave the prior application untouched. Publication failure is shown
separately from the compiler exit; inspect actual storage if publication is
uncertain. CLOSE removes unconsumed staging, closes the master, reaps the complete
family and invalidates the handle. A new job can reuse slots, never the old PID.
Creator exit/fault also performs family cleanup. CLOSE reports a staging-cleanup error only after unconditional task/descriptor reclamation and handle invalidation; inspect storage rather than retrying the stale handle.

Publication is not a new save transaction or durability guarantee. It does not
fsync the executable/parent automatically. Use ordinary `sync`/clean shutdown for
the separately tested retention path. Arbitrary concurrent writers, device faults
during publication and power loss require separate acceptance.

## Logs and retention

Slave stdout/stderr share one real PTY stream. The view accurately labels it
**merged stdout/stderr**. Raw bytes are retained; diagnostic colors are disabled
by the fixed GCC option. Display sanitizes controls/non-ASCII to `?`, ignores CR
and wraps at 50 columns; there is no inferred per-stream origin or ANSI support.

Four fixed records each retain the newest 8,192 raw bytes. Appending evicts the
oldest byte at capacity and increments the displayed dropped-byte count, including
partial-line truncation. A fifth attempt evicts the oldest record and increments
the visible record-eviction count. Task metadata, actual exit reason/code,
publication outcome and remaining logs belong to that record, not a reused task
slot. Previous attempts remain selectable while a newer task runs or finishes.

Each event-loop turn handles at most 16 GUI replies/events, reads four 1,024-byte
batches, and advances one acknowledged GUI upload. A frame renders at most 12 log
lines after scanning at most the fixed 8 KiB record twice. Log storage has no heap
growth; raster snapshots stay immutable while uploaded. The launcher now shares
the existing asynchronous terminal/files transport without changing the GUI ABI.

| Boundary | Implemented retention |
| --- | --- |
| Build completes | Record/log tail retained; job resources closed/reaped |
| Terminal closes/restarts | Launcher records unaffected |
| Compositor or launcher-requested session replacement | Same launcher process retains records and an active job; finite PTY buffering provides backpressure during reconnect |
| Launcher exit/fault, or a new launcher process | Records lost; owned job family closed/reaped |
| System reboot | Records lost; only explicitly saved filesystem artifacts can remain |

No persistent log files or unsaved-buffer crash recovery are promised. Runtime
quotas remain 16 MiB heap, 32 task slots, 32 FDs/process, 128 shared descriptions,
four PTYs and four GUI windows. Global resource panels, general task termination,
broader service recovery, arbitrary build recipes and M17.5 remain later work.

## Experimental native build v1

`0x104` is independent of frozen native v1, native process-control `0x101`,
interactive PTY `0x102`, file publication `0x103` and the GUI C ABI. RDI contains
`(1 << 32) | operation`; RSI is zero for START or the exact owned job PID; RDX
points to exactly 32 copied bytes. Little-endian u64 fields are buffer address,
length, reserved-zero, reserved-zero. Layout is 32 bytes, alignment 8.

| Operation | Checked request and result |
| --- | --- |
| 0 START | Buffer/length/reserved all zero; return job PID |
| 1 READ | Length 0–1024, reserved zero; partial bytes, WOULD_BLOCK, or zero after drained EOF |
| 2 STATUS | Length exactly 16, reserved zero; copy existing `WaitStatus`, WOULD_BLOCK while live |
| 3 CLOSE | Buffer/length/reserved zero; close/reap/invalidate the exact owned job |

A zero-length READ returns zero without transferring data; only a positive-length READ returning zero indicates EOF.

Malformed version/op/reserved/length/START ID returns INVALID_ARGUMENT. Checked
user access rejects inaccessible/cross-page/readonly spans without consuming a
pending result. Foreign/stale/cross-interface handles return NO_CHILD. Active
controlled job returns BUSY; PTY/open-description exhaustion returns FILE_LIMIT;
task exhaustion returns PROCESS_LIMIT. Missing/unadmitted compiler and filesystem
reservation errors retain their real errors. No native-v1/GUI layout is extended.

PTY writes, poll readiness and blocked-writer wakeups share the same CRLF/echo
reservation and input-EOF predicates. With one output byte free, a writer that
reserves two bytes remains blocked until the reader creates enough space. This
prevents a false-ready loop from starving the nonpreemptible kernel under an
unthrottled producer; capacities and write policy are unchanged.

Native launcher/Files/terminal connections retain the existing 100-tick connection window for asynchronous slot reclamation. A genuinely full four-client quota still returns NO_SPACE; a stopped peer can release its slot during that bounded wait. This does not increase the GUI quota or the three-replacement limit.
