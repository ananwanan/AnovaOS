# M7 persistent storage and source editing

This document defines the PCI/virtio DMA boundary, supported ext2 profile,
native durability calls, image lifecycle and serial editor. Evidence is in
[validation/M7.md](validation/M7.md). The native process and VFS contracts from
[M5](M5_PROCESSES.md) and [M6](M6_FILESYSTEM.md) continue to apply.

## Devices, mappings and ownership

The reference machine is QEMU 8.2.2 q35/TCG, one BSP, qemu64 and 256 MiB RAM.
PCI configuration mechanism 1 enumerates bus 0 and configured downstream
bridges, including multifunction devices. The driver accepts exactly one
non-transitional `1af4:1042` virtio-blk function. It validates the capability
chain, common/notification/device regions, BAR sizes and notification bounds.
BAR probing disables decoding/mastering and restores the BARs before decoding.
The early mapper checks that device pages do not overlap RAM/firmware, then
maps them supervisor-only, UC and NX before M3 imports the page tables.

The device negotiates VERSION_1 and FLUSH, and acknowledges read-only when
offered. Packed rings, indirect descriptors, EVENT_IDX and multiqueue are not negotiated.
PCI MSI/MSI-X are explicitly disabled/masked, in addition to disabling INTx;
configuration and queue vector fields are set to NO_VECTOR. A split queue contains eight descriptors, with exactly one
request outstanding. Its descriptor/available/used areas, request header,
512-byte bounce buffer and status occupy one zeroed, physically contiguous,
DMA32 frame. `Owner::Dma` records its lifetime independently of the heap and
user address spaces. No user pointer or borrowed stack buffer is given to DMA.

Volatile fields and explicit x86 coherent-DMA ordering fences publish descriptors
before the available index/notification and acquire completion before copying
data. Completion checks the used index, descriptor head, written length and
device status. Sector bounds and overflow are checked before submission; the
last valid sector is read during attach. FLUSH is a real virtio request.
Read-only, I/O error, unsupported operation and timeout are explicit failures.

Completion and reset use the ACPI PM timer, with a two-second bound independent
of delivered LAPIC ticks; kernel polling runs at IF=0. No spinlock or heap lock
is held during I/O. Reset acknowledgement precedes DMA release. If reset cannot
be confirmed, bus mastering is disabled and the frame is quarantined instead
of being recycled. Device IRQs remain masked. Polling can delay user processes
and coalesce delivered timer ticks; this is not an asynchronous block layer.

## Mounted ext2 profile

The read-only initramfs remains `/`; tmpfs remains `/tmp`; devfs remains `/dev`.
The disk's filesystem root is attached at `/persist`. The whole raw image is
ext2, without a partition table; the ISO/ESP and writable data image are separate.

| Property | Admitted implementation |
| --- | --- |
| Revision / layout | Revision 1, one block group, 4 KiB blocks/fragments, first data block 0 |
| Inodes | 128-byte inodes, first ordinary inode 11, at most 4,096 inodes |
| Volume | 64–32,768 blocks within device capacity; tool creates 16 MiB with 128 native or 512 Linux-development inodes |
| Metadata | Block bitmap 2, inode bitmap 3, inode table 4, contiguous reserved metadata |
| Features | COMPAT=0, INCOMPAT=FILETYPE only (2), RO_COMPAT=0 |
| Objects | Regular files and directories; no ext2 links, hard links, xattrs, ACLs or devices |
| Files | Direct and one single-indirect block; at most 1 MiB per file |
| Directories | Existing directories up to 12 direct blocks; new directories one block; no directory growth/removal |
| Namespace | Existing VFS limits: 96 total nodes, 63-byte names, 128-byte paths; disk import depth at most eight |
| Editor | 16 KiB ASCII buffer and canonical terminal lines up to 255 bytes |

Unknown features, unsupported inode flags/types, dirty volumes, impossible
geometry/counters, truncated or malformed directory records, unallocated data
pointers, metadata aliases, duplicate blocks and pointers beyond EOF fail.
No ext3 journal, ext4 extents/64-bit/checksums, sparse-super, large-file feature,
resize inode or feature downgrade is attempted. A rejected disk leaves the
initramfs shell running with an unavailable, read-only `/persist` mount point.

The namespace is imported at boot; file data stays on disk. Read, write,
append, seek, truncate, mkdir and unlink use the ext2 inode. Open descriptions
retain shared offsets across dup/fork/spawn/exec. An unlinked file remains
readable through its existing opens; final close releases its inode/blocks.
Fallible block scratch buffers use the heap, keeping the normal 64 KiB kernel
stack unchanged. ELF loading makes one fallibly allocated owned byte copy.

`/persist/.anova-abi` must contain exactly `anova.native.v1\n`. This explicit
image manifest selects the native personality for executable-mode disk files.
`install` sets those execute bits after copying. The ELF loader still validates
ET_EXEC, segments and permissions. ELF OSABI is never used to guess Linux/native
identity. This disk is a native-v1 image; Linux executables need M8's separate
personality and acceptance policy.

## Ordering, errors and shutdown

Before the first mutation, the primary superblock's clean bit is cleared and
flushed. New blocks are zeroed before pointer publication. File data is flushed
before the inode exposes a larger size; new inodes/bitmaps are flushed before
publishing directory entries. Truncate first removes inode references and
flushes, then releases blocks. Unlink first removes the directory reference
and flushes; the final close truncates/releases the unlinked inode.

There is no dirty data cache. `sync`/`fsync` issue a device FLUSH covering earlier
data and metadata. A failed mutation after any published write poisons further
filesystem I/O, including failures to allocate scratch memory; it cannot later
claim a clean shutdown. Capacity prechecks reject ordinary full-file/full-disk
writes without destroying earlier content. Partial hardware-write failure is
not rolled back or hidden behind success.

`shutdown` first flushes while descriptors remain intact, so an already failed
disk returns an error without disabling the terminal. It then stops process
writers, closes descriptions (including open orphans),
flushes storage, records/flushes a clean superblock, and confirms device reset
before releasing DMA. A late unmount/reset failure reports a diagnostic and
halts without a power-off success marker. Only successful teardown requests ACPI S5. The power helper admits
the pinned QEMU firmware's legacy System-I/O PM1a register and constant `_S5`
package; it is not an AML interpreter or a promise of physical-machine power
management. The runner requires an actual successful QEMU exit, not just a log
message or a halted CPU. No successful power-off marker is emitted on a flush
or reset failure.

**ext2 is not journaled.** Forced termination/power loss during updates may
leave inconsistent metadata, leaked allocations or partially overwritten data.
Dirty images are rejected; recovery requires an offline host `e2fsck` and a
supported clean profile. Atomic replacement, rename, crash recovery and
arbitrary power-loss durability are not implemented. File timestamps and
permission identities are also not a usable POSIX service yet.

## Additive native v1 interfaces

Registers, checked user copying and negative native errors remain as in M6.

| Number / bit | Interface | Behavior |
| --- | --- | --- |
| syscall 20 | `sync()` | FLUSH the mounted disk, 0 or a real error |
| syscall 21 | `fsync(fd)` | Validate a regular/directory target and flush the volume; pipes/devices fail |
| syscall 22 | `shutdown()` | Success does not return; clean unmount/reset/S5 on the reference platform |
| syscall 23 | `set_executable(path)` | Set native disk file execute bits; directories/read-only files fail |
| open bit 64 | `OPEN_EXCLUSIVE` | Requires CREATE; existing paths return ALREADY_EXISTS before mutation |
| errors -21/-22/-23 | IO_ERROR / INVALID_FILESYSTEM / DEVICE_TIMEOUT | Distinct native errors, not Linux errno |

Rust wrappers are in `anova-rt`; C callers can use `anova_syscall` with these
documented numbers. No permission/user model exists; shutdown is currently an
unprivileged native operation in this single-user development system.

## Image and editor workflow

```sh
cargo xtask root-image build/disks/work.ext2
cargo xtask run --disk build/disks/work.ext2
```

`root-image` creates a new regular 16 MiB image, formats the explicit profile,
adds the manifest, `/src/hello.c` and native `/bin/hello-c`, and marks it clean.
It refuses to overwrite an existing path. User-selected images must be direct
regular files under `build/disks/`; symlinks and real block devices are rejected.
Plain `cargo xtask run` instead creates a fresh temporary image and deletes it
with that session's firmware variables. It never implicitly reformats an
existing persistent image. `shutdown` exits QEMU normally; Ctrl+C is a forced
termination and can leave a persistent image dirty.

Inside the guest:

```text
edit /persist/src/demo.c
a
int main(void) { return 42; }
.
w
q
install /bin/hello-c /persist/demo
/persist/demo saved-native-app
status
sync
shutdown
```

The editor opens existing text or a new path. `p` prints, `a` appends lines until
a single `.`, `d N` deletes a line and `c N` replaces it with the next input line.
`w` saves and fsyncs, `q` refuses unsaved changes and `q!` discards them. Errors
retain the in-memory editing buffer. Saving currently truncates then writes,
so failed/interrupted save can partially replace the old disk file; there is no
atomic save claim. `cp SOURCE DEST` copies a new file; `install SOURCE DEST`
also sets native execute bits. Both use exclusive creation and refuse existing
destinations, including aliases of the source.

`cargo xtask persistence-test` drives actual serial editing and native syscall
checks, reopens the same image across normal shutdown/reboot, compares source
bytes and ELF SHA-256 on the host, runs `e2fsck -fn`, and exercises read-only,
bad-feature and blkdebug I/O failures. Its retained image is separate from every
default/test temporary disk. This establishes storage and editing, not guest
GCC/Rust compilation; those remain M11/M12.

## Specification references

- [OASIS Virtio 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html),
  device lifecycle, split queues, PCI transport and block requests.
- [Ext2 on-disk description](https://www.nongnu.org/ext2-doc/ext2.html) and
  [kernel ext2 documentation](https://docs.kernel.org/filesystems/ext2.html).
- [ACPI hardware registers](https://uefi.org/specs/ACPI/6.5/04_ACPI_Hardware_Specification.html)
  and the constant `_S5` package contract.
- [QEMU blkdebug](https://www.qemu.org/docs/master/devel/testing/blkdebug.html),
  used only by the host error-injection acceptance scenarios.

No third-party driver/filesystem implementation was imported.
