<!-- Define the bounded graphical file/editor workflow, shared namespace and explicit save failure contract. -->
# M17.3: files and reliable source editing

**English** | [简体中文](../../zh-CN/milestones/M17_FILES.md)

This implements the minimal file/editor loop in [M17](M17_DESKTOP.md), following the
accepted [PTY terminal](M17_PTY.md). Implementation and executed local checks are
recorded separately in [M17.3 validation](../validation/M17.3.md). The bounded real
GUI storage-fault profile is locally accepted; M17.5 graphical compiler workflows
remain separate.

## Shared work disk and launch

The native file/editor client and Linux-musl PTY shell use the same mounted VFS
`/persist`. A file saved as `/persist/project/main.c` is readable by the graphical
terminal at exactly that path. This is a shared filesystem, without a host sync
folder or a second editor-private store. Sources are ordinary data. Executable
admission still follows the immutable disk ABI marker: select a fresh Linux work
disk when compiler outputs will be executed by the PTY shell.

```sh
cargo xtask desktop-image
cargo xtask ext4-journal-image --linux build/disks/editor-work.ext4
cargo xtask run --desktop --disk build/disks/editor-work.ext4
cargo xtask desktop-files-test
cargo xtask desktop-files-fault-test
# Reproduce one fault case independently.
cargo xtask desktop-files-fault-test --case postpublish-fsync
# Optional: reuse the existing verified M11 guest compiler seed.
cargo xtask run --desktop --toolchain --disk build/disks/editor-work.ext4
```

Use clean guest shutdown for the named persistent image. The ordinary desktop
and serial recovery shell remain independently available. Existing desktop and
PTY regression commands remain `desktop-test` and `desktop-pty-test`.

## Controls

Focus the launcher and press `f`; Files replaces the C++ demo slot. The PTY
terminal remains on `t`. The Files window is 318×240 at (270,250); focus follows
the actual window record.

| Key | Action |
| --- | --- |
| Up/Down, Enter, Backspace | Select a directory entry, open/enter it, visit the parent |
| F1 | Open an absolute file path or navigate to a directory |
| F2 / F3 | New source path / rename the opened file in editor view |
| F4 | Close with an unsaved-buffer check |
| F5 or Ctrl+S | Save; require explicit overwrite if the baseline changed |
| F6 | Delete the opened file in editor view after `y` confirmation |
| F7 / F8 | Return to directory view / create a directory |
| Enter / Esc in a path prompt | Submit / cancel |
| `s` / `d` / Esc in an unsaved prompt | Save and continue / discard and continue / cancel |
| `y` / Esc in an overwrite prompt | Confirm overwrite / cancel |

The launcher requires Files to close before session stop/restart/exit, preserving
the editor's opportunity to check unsaved work. This does not claim unsaved-buffer
recovery after a client crash, compositor loss or host termination.

## Buffer and path bounds

One source buffer holds at most 16,384 exact bytes. Printable ASCII, tab, CR and
LF are supported; unsupported binary bytes and oversized input are rejected
without replacing a usable buffer. Paths are absolute and lexically rooted at `/persist`,
and at most 120 bytes; intermediate components are at most 63 bytes and the
editable basename is at most 55 bytes, reserving eight bytes for the recovery
suffix. The directory view retains at most 64 entries; F1 opens an exact path
beyond that view.
Traversal, invalid names and reserved transaction suffixes are rejected.
Parent symlinks retain ordinary VFS resolution; this lexical policy is not a
filesystem security sandbox. The
buffer and directory view have fixed bounds; filesystem capacity is not expanded.
The inherited bitmap font draws Latin letters uppercase while stored bytes retain
case. The source view follows the cursor across 16 rows and 50 columns; tabs use
a single blank display cell.

A successful load/save establishes an exact baseline. Editing marks the buffer
dirty; file switches, new buffers, removal and close require an explicit unsaved
work decision. Saving over a file changed through another process requires an
explicit overwrite decision. This is a preflight snapshot check, without an
exclusive lock spanning all save syscalls; concurrent writers must coordinate
to avoid a modification after that check. Failed saves retain the dirty bytes and report the
actual operation error; an error never silently marks the buffer clean.

## Save and prior-copy policy

A save exclusively creates a sibling `.m17-tmp`, writes every byte, fsyncs that
file and checks close. File and directory fsync currently flush the shared
filesystem/device globally; they do not supply independent per-directory journals. For an existing target, it also exclusively creates and
fsyncs a `.m17-bak` copy of the exact current bytes and persists the parent before
publication. The fully prepared temporary file is renamed over the target, then
the parent is fsynced. Only success of the required operations clears dirty state.

An error before publication preserves the old target. A required-save error after
replacement retains the backup and dirty buffer because publication or persistence
may be uncertain. Existing transaction/recovery files cause an explicit error;
they are never silently truncated. After success, backup cleanup is best effort;
a cleanup error can leave the usable recovery copy, or removal may already have
completed before a later error. The buffer stays clean because required save
operations completed; the GUI reports `Saved; backup cleanup failed: inspect storage`
and logs `backup_cleanup_failed=true`. It keeps that outcome visible instead of
replacing it with an error from an unnecessary directory refresh. Inspect actual
artifacts before explicitly removing any retained copies and retrying.

On a usable mount, or after ordinary mount recovery on a separate copy, inspect
both siblings in the graphical PTY. Restore selected bytes to a different path
before removing evidence:

```sh
/linux/cat /persist/src/main.c.m17-bak
/linux/cat /persist/src/main.c.m17-tmp
/linux/cat /persist/src/main.c.m17-bak > /persist/src/recovered.c
/linux/cat /persist/src/recovered.c
```

After checking the restored file, use the independent COM1 native shell to remove
only the transaction artifacts that you have inspected, then retry the editor:

```sh
rm /persist/src/main.c.m17-tmp
rm /persist/src/main.c.m17-bak
```

The selected graphical BusyBox profile has no `rm` or `mv`; recovery uses existing
`cat`/shell redirection and the native COM1 `rm` command. An absent sibling is an
ordinary missing-file error; do not infer that every failed phase created both.

Allocation exhaustion and uncertain device completion have different recovery
boundaries. Once real inode/block pressure is released, inspect and explicitly
remove this transaction's siblings before retrying the still-dirty buffer. After
a completed device write/flush error, the journal backend deliberately fails the
mount closed. Removing an external fault does not clear that mount state; a
same-mount retry still reports an error and preserves the buffer and its prior
dirty/clean state. Ordinary mount recovery requires a fresh boot. The fault gate preserves the failed image,
boots a separate copy and explicitly re-enters the observed pending bytes before
retrying. It does not claim crash recovery of an unsaved editor buffer.

These are checked syscall guarantees on the implemented ext2/ext4 backends.
Rename/fsync success does not establish arbitrary atomic durability or power-loss
recovery. The declared GUI fault cases and clean ACPI shutdown/fresh QEMU boot
of the same disk have separate [executed evidence](../validation/M17.3.md).
No in-guest reboot command is added. In-save power cuts and failure points beyond
that matrix remain unverified. This does not repeat or supersede M16's gates.
