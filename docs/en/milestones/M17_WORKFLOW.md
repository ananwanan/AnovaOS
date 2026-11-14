<!-- Define the desktop monitor and fixed three-language build, publication and recovery profiles. -->
# M17 desktop monitor and compiler workflow

**English** | [简体中文](../../zh-CN/milestones/M17_WORKFLOW.md)

## Resources, tasks and services

In the launcher, press `m` and choose Resources, Tasks or Services with `1`, `2`
or `3`. Resources samples the managed frame pool, actual allocator heap,
scheduler states, occupied VFS entries and backing slots, open descriptions,
PTY master allocations, GUI reservations and mounted work volume. Connected
GUI clients can be fewer than occupied reservations while a compositor processes
disconnects. The frame pool includes allocator reservations and excludes
memory outside the managed regions. The heap reports allocated buddy bytes.
Disk counts include filesystem metadata and the journal. A failed mount
withholds its free counts. Samples arrive no more often than every 50 delivered
timer ticks while the panel is visible. CPU time, per-task RSS and instantaneous
peaks are unavailable. Existing scheduler-boundary and storage-mutation peak
reports retain their separate scopes.

The versioned native monitor uses `0x105` with RDI `(1 << 32) | operation`.
Operation 0 copies one 176-byte `Resources` record. Operation 1 copies 32
64-byte `Task` records, zeroes unused rows and returns the visible count.
RSI names the output and RDX gives its exact length. Operation 2 accepts an
exact PID in RSI and requires zero RDX. It returns 0 for termination or 1
for a previously exited owned native child. The attempt leaves status available
to ordinary wait. See the [wire records](../../../crates/abi/src/monitor.rs).

Every native caller can read global counts. Task records expose only self,
direct children, threads of visible Linux children and tagged PTY families
owned by the exact creator. Rows distinguish a process ID, parent, thread-group
leader and a PTY family. Start time is a coarse tick from creation. A zombie
retains its original personality and real exit status until its owner waits
or the scheduler reaps it. Stop accepts only native direct child leaders.
Nonchildren, stale PIDs, Linux processes and threads are rejected. Native
descendants follow the existing reparenting rules. A tagged build is cancelled
as a family through its own owner interface. The launcher retains the most
recent observed client or service exit and displays stop errors.

Files and Terminal close cooperatively with `F4` in their own window. Files
uses its existing dirty-buffer dialog for explicit closure. The existing
launcher key `3` can also terminate Terminal and reap its tagged PTY family.
Services `c` replaces the compositor, `r` restarts the session, `q` closes the desktop
and `p` requests the existing clean shutdown. Full session restart, exit and
shutdown require Files to close first. Graphical shutdown also requires any active
build to finish or be cancelled. Compositor replacement and recovery reuse
the same three-replacement limit. The launcher shows application launch errors
and the Services page shows when the replacement allowance is exhausted.
A live Files process reconnects with the same editor buffer and a live Terminal
process reconnects with its PTY and history.
Other clients start again with fresh handles. Loss of the client process or
exhaustion of the replacement path can still lose unsaved bytes. COM1 remains
independent.

## Fixed development recipes

Use a writable Linux-personality work disk. `--toolchain` admits GCC/G++ 11.2.0
and `--rust` adds the checked Rust 1.90.0 tools and matching musl std to the
desktop image.

```sh
cargo xtask fetch
cargo xtask toolchain-build
cargo xtask rust-build
cargo xtask ext4-journal-image --linux build/disks/development.ext4
cargo xtask run --desktop --rust --disk build/disks/development.ext4
```

Create or edit sources with Files. In the launcher, press `b` and choose a
recipe. In the graphical terminal (`t` in the launcher), execute the published
path and inspect `$?` for its exit code. Use Services to shut down after saving.

| Key | Recipes and exact source paths | Published path |
| --- | --- | --- |
| `g` | GCC with `/persist/build.c` | `/persist/build-app` |
| `c` | G++ in C++11 mode with `/persist/build.cpp` and `/persist/helper.cpp` | `/persist/cpp-app` |
| `r` | Rust 2024 with `/persist/build.rs` | `/persist/rust-app` |

Build recipe v1 uses `0x106` and the [fixed argument lists](../../../crates/abi/src/workflow.rs).
START takes a recipe ID in RSI and reserves that recipe's `.next` path
exclusively. READ, STATUS and CLOSE keep the existing 32-byte build request
and target an exact leader PID. CANCEL with a zeroed record terminates the
tagged family and preserves the leader's signal status for capture. PUBLISH
with a zeroed record requires the owner, an actual zero exit, drained EOF,
no live family members and nonempty linker output. Close reaps
the family and releases resources even if staging cleanup reports a device
error. Both build interfaces share one global job and the four PTYs. Native
v1 and the existing GUI, process, PTY, file and legacy build records retain
their layouts and semantics.

The compiler writes to an exclusively reserved volatile path within the existing
1 MiB tmpfs per-file limit. This avoids a JBD2 transaction for each of the linker's
small writes. PUBLISH copies up to 64 KiB per call to persistent staging, returns
WOULD_BLOCK after a successful chunk and then requests the checked rename/replace.
The source, persistent staging and published file share the same work disk.
An occupied temporary or staging path requires inspection. These fixed paths remain
in the ordinary shared VFS namespace. The kernel pins both reserved file
identities and refuses to publish or remove a file that replaced either path.
The owner must coordinate edits from other writers with an active job.
Publication reports separately from compile success, and clean
shutdown applies the existing sync/unmount contract. Device errors can leave
a fail-closed mount and cleanup evidence. Power-loss guarantees follow the
separately tested filesystem profile.

The launcher keeps four 8 KiB merged stdout/stderr tails with attempt and
leader identities. It consumes up to sixteen GUI events, four 1 KiB transfers
and one acknowledged upload per turn. `x` requests cancellation, `[` and `]`
select attempts, PgUp and PgDn scroll, and Esc returns to the launcher.
Logs are retained inside the live launcher across compositor replacement.
The graphical terminal uploads changed cell rows and old/new cursor rows,
using the same single acknowledged upload per turn. Geometry changes and
reconnection redraw the full surface.

See [M17.4 validation](../validation/M17.4.md) for resource and service evidence
and [M17.5 validation](../validation/M17.5.md) for the three-language gate.
