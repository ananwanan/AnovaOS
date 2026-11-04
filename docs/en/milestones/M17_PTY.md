<!-- Define native PTY ownership, admitted Linux tools, incremental GUI transport and bounded ANSI behavior. -->
# M17.2: real graphical PTY terminal

**English** | [简体中文](../../zh-CN/milestones/M17_PTY.md)

This implements the terminal contract in [M17](M17_DESKTOP.md). Local evidence and remaining
limits are recorded separately in [M17.2 validation](../validation/M17.2.md).

## Launch and executable admission

The opt-in desktop archive admits `/bin/terminal` as `anova.native.v1`, and a separately built
BusyBox 1.36.1/musl 1.2.4 ash as `linux.x86_64.v1`. Its source commit remains
`1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4`. The terminal configuration adds
`CONFIG_ASH_JOB_CONTROL=y`; the original M8/M9 BusyBox configuration/build is unchanged.
Keep `build/images/busybox-terminal-1.36.1-source.tar.gz`, which includes the actual config,
when redistributing the desktop ISO. Its license is also in `/etc/busybox.LICENSE`.

Focus the launcher and press `t`. The terminal replaces the third application slot and
retains the four-window limit. Its initial surface is 312×232 at (450,20), with a 50×26
cell grid. `F4` closes it; `F5` closes the current PTY family and starts a fresh shell.
Shell exit/EOF leaves the window available for explicit restart. Shift-PgUp/PgDn moves
through history in ten-line steps; ordinary input returns to the live view.

```sh
cargo xtask desktop-image
cargo xtask run --desktop
# Build the existing source-locked M11 toolchain first, if it is not available:
cargo xtask toolchain-build
cargo xtask ext4-journal-image --linux build/disks/pty-work.ext4
cargo xtask terminal-image --toolchain
cargo xtask run --desktop --toolchain --disk build/disks/pty-work.ext4
cargo xtask desktop-pty-test
```

The default desktop includes the PTY shell but does not download compiler seeds. The optional
compiler archive reuses M11's complete recipe/stage verification and explicit Linux entries;
it is not a compiler reconstruction claim. `/bin/gcc` selects the existing static tool profile.
For example, `/bin/gcc /etc/terminal-hello.c -o /persist/pty-app`, followed by
`/persist/pty-app; /linux/echo RUN-$?`, compiles and runs a real C application.

Native GUI executables stay in the boot archive. A native-personality disk can hold sources
and logs, but its executable marker does not admit Linux compiler outputs. Select a new
Linux-personality work disk for those outputs. The mounted `.anova-abi` marker remains
immutable; no suffix, pathname or ELF header overrides admission. No GUI authority is
granted to the Linux shell/compiler. COM1 remains a separately supervised native shell.

## Experimental native PTY interface

`crates/abi/src/pty.rs` defines syscall `0x102`, separate from frozen native v1, the GUI C ABI
and native process-control `0x101`. RDI is `(1 << 32) | operation`, RSI is an exact session
ID (zero for START), RDX points to a fully copied 32-byte record. Fields are little-endian:

| Offset | Field | Meaning |
| --- | --- | --- |
| 0 | buffer, u64 | User transfer/status address, otherwise zero |
| 8 | length, u64 | Transfer bytes (0–1024) or exactly 16 for STATUS |
| 16 | rows, u32 | START/RESIZE, 1–128; otherwise zero |
| 20 | columns, u32 | START/RESIZE, 1–256; otherwise zero |
| 24 | reserved, u64 | Must be zero |

| Operation | Result and lifetime |
| --- | --- |
| 0 START | Allocate a real free PTY and admitted interactive Linux shell; return its never-reused PID/session ID |
| 1 READ | Nonblocking partial output, WOULD_BLOCK while empty/live, zero after slave EOF and buffered bytes are drained |
| 2 WRITE | Nonblocking partial input through the actual line discipline, or WOULD_BLOCK |
| 3 RESIZE | Update winsize and generate SIGWINCH for the real foreground group |
| 4 STATUS | Copy the existing native 16-byte WaitStatus without reaping; WOULD_BLOCK while the shell lives |
| 5 CLOSE | Close the master, terminate/reap the complete tagged family and invalidate the handle |

Only the exact creator can operate a handle. The master lives in a private kernel descriptor
table and is never inherited by native fork/spawn. The shell gets only slave descriptors
0/1/2, its own SID/PGID, a bounded PATH/TERM/PS1/HOME environment, and the creator's cwd.
Linux `/dev/tty` resolves to that session's controlling slave. Shell stdout/stderr share
the same PTY output queue; bytes alone do not reveal their original stream identity.

Linux fork/clone descendants retain an independent membership tag across exec and setsid.
Creator exit, fault or supervised termination closes/reaps that family, including detached
descendants, without targeting unrelated tasks. It is immediate teardown, not an unsaved-work
dialog or a global task-management API. START allocation failure rolls back its master/child.
READ retains queued bytes after failed user copying; STATUS copy failure retains status.
Native `0x101` continues to reject live Linux children.

The selected ash requires WUNTRACED. The Linux adapter now reports an actual child's stopped
status once, after successful copying, and wakes blocked waits; WNOHANG combines with it.
Continue clears obsolete stop notifications. This does not add WCONTINUED, wait-by-group,
complete multithreaded job control or background TTIN/TTOU enforcement.

## Event loop and storage bounds

Every UI turn processes at most 16 GUI replies/events, submits one queued input batch,
reads at most four 1,024-byte output batches, and advances one acknowledged GUI upload.
The UI never blocks on slave I/O, child wait or a whole-window presentation. A private
raster snapshot remains unchanged until PRESENT; resize invalidates an obsolete upload
and redraws the new dimensions. Focus/input reset discards pending local key bytes.
COM1 diagnostic records are independent of PTY output and are not a second stream capture.

Canonical lines retain the existing 255-byte payload limit plus newline. Input storage is 256 bytes; partial writes retain the remainder and a full local queue is
diagnosed. Ctrl+C drops unsubmitted local bytes and sends the real interrupt character.
Ctrl+D sends the real EOF character; Backspace uses the line discipline's erase character.
Printable US set-1 input is supported. Arrow/Home/End/Delete sequences are forwarded for
programs that use raw input; the shell's canonical editing profile supports Backspace,
not a full readline interface. The finite presentation/input queues can still reject an
unbounded physical-input flood; there is no lossless input claim beyond their capacities.

The cell model uses at most 52×26 visible cells plus exactly 256 saved lines, under 45,000
bytes with no heap growth. The private raster is at most 320×240×4 bytes. Resize preserves
the top-left intersection, clips/pads without reflow and clamps the cursor. New output
evicts the oldest saved line at capacity. History is volatile across terminal exit.

Global capacities remain 16 MiB heap, 32 task slots, 32 FDs per process, 128 shared open
descriptions, 2,048 VFS slots and four PTYs/windows. A steady launcher/compositor, two GUI
peers, terminal and Linux shell share those limits with init and COM1. Compiler/build peaks
are a separate measured workload. Kernel-boundary observations are not exact RSS/CPU time.

## ANSI subset

The stream parser retains state across every read boundary and allocates nothing.

| Input | Implemented behavior |
| --- | --- |
| CR, LF, BS, TAB, BEL | Carriage return, line advance/scroll, backspace, eight-column tab, silent bell |
| Printable ASCII | Cell output with delayed wrap; unsupported UTF-8 scalars use `?` |
| CSI A/B/C/D, G, H/f | Clamped cursor movement/position, normal default parameters |
| CSI J/K with 0/1/2 | Erase display/line sections, without cursor homing |
| CSI m | Reset, bold/normal, inverse/normal, basic/bright 16 colors and default foreground/background |
| CSI s/u, ESC 7/8 | Save/restore one clamped cursor position |
| CSI ?25h/l | Show/hide cursor |
| OSC/DCS/PM/APC | Consume/discard through BEL or ST; no title, clipboard or arbitrary control side effects |

CSI accepts at most eight parameters of four digits. Malformed/oversized sequences are
discarded through their final byte; a new ESC or CAN/SUB can cancel an incomplete CSI.
An SGR containing an unsupported parameter is discarded as a whole, including extended
color operands. Alternate screen, scroll regions, insert/delete lines, terminal queries,
full xterm compatibility, Unicode shaping/IME and terminal-based full-screen editors remain
outside this profile. Font rasterization reuses the original GUI bitmap font, whose Latin
letters display in uppercase. The GUI C/C++/Rust ABI and record layouts are unchanged.

Specification references: [XTerm control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html),
[Linux terminal sizes](https://man7.org/linux/man-pages/man2/TIOCSWINSZ.2const.html) and
[waitpid](https://man7.org/linux/man-pages/man2/waitpid.2.html). No third-party terminal emulator
implementation is imported.
