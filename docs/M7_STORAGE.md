# M7 storage and native editor: implementation boundary

This document describes the incremental M7 feature branch. It is **not** a claim
of M7 acceptance. The base M6 kernel still boots without a block device; its
initramfs is immutable and /tmp does not survive a reboot.

## Implemented foundations

- `user/editor` is a native, bounded 8 KiB line editor. The `ed PATH` alias
  resolves to its separately built native ELF. Commands: `p` to print,
  `a TEXT` to append, `r LINE TEXT` to replace, `d LINE` to delete, `w` to
  save, `q` to exit without dirty edits, `Q` to discard changes. It uses
  actual VFS open/read/write/truncate and does not emulate persistence.
- `anova-storage::block` checks sector capacity/overflow before touching the
  device, supports checked unaligned 512-byte sector reads and read-modify-write,
  and requires an explicit device-provided flush result.
- `anova-storage::pci` scans bus/device/function for modern virtio-blk
  (vendor 1af4, device 1042), parses bounded PCI vendor-capability chains and
  rejects malformed/duplicate common, notify, ISR or device structures.
- `anova-storage::virtio` defines modern-only feature negotiation and split-ring
  request/completion layouts. It intentionally does not pretend that an unused
  descriptor definition is a functioning hardware driver.
- `anova-storage::ext2` validates the primary superblock, one block group,
  descriptor, supported feature flags and direct-block file type. It can find
  immediate root entries, read regular files and replace the contents of
  **existing, preallocated regular files** within their direct blocks. The write
  path updates the inode size after data writes and requires `flush()` success.

## Explicit exclusions

The live kernel is **not connected to PCI BARs, virtqueues or a DMA allocator**,
and `anova-storage` is not yet a kernel dependency. There is no writable ext2
VFS mount, no persistent root, no disk data path or ext2 file creation,
allocation, indirect block handling, directories beyond root, chmod, rename,
unlink or timestamps. A file that needs an additional data block fails instead
of allocating from a bitmap. The strict writer excludes journals and unknown
feature flags, large/indirect files and multi-group images.

Only a completed physical transport may acknowledge FLUSH, and its queue
must retain DMA ownership until a matching completion. A read-only ISO cannot
serve as the persistence test disk. No ext2 implementation can guarantee
crash-consistent metadata updates without additional recovery.

## Next implementation sequence

1. Supply checked PCI BAR mapping and a bounded DMA frame API with explicit
   ownership, low-memory constraints and cache behavior; detect a modern
   virtio-blk device without exposing the MMIO window to Ring 3.
2. Negotiate queue 0 and FEATURES_OK/DRIVER_OK; implement three-descriptor,
   one-outstanding-request split ring and bounded polled completion. Add capacity,
   status, timeout and flush rejection paths. Leave external device IRQs masked.
3. Extend ext2 with carefully ordered block/inode bitmap allocation and file
   creation, small directory insertion, data growth, fsync and dirty/clean state;
   reject unsupported on-disk features instead of interpreting them loosely.
4. Expose a `/disk` VFS mount and sync/fsync native syscalls with parent/fork
   shared offset semantics. Keep `/tmp` volatile.
5. Add host-created ext2 test images, attach explicitly under QEMU, prove that
   save + controlled shutdown + second boot compares identical text/binary hashes;
   test corrupt superblocks, unsupported flags, I/O beyond capacity, read-only
   devices, write errors and disabled flush. Re-run all M0–M6 negative cases.

## Proposed acceptance commands (not yet implemented)

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
cargo xtask check-files
cargo xtask test-host
cargo xtask check-apps
cargo xtask verify
cargo xtask terminal-test
# Future: cargo xtask disk-create and cargo xtask disk-test
```

See [M7 evidence](validation/M7.md) for actual execution status, not a
hypothetical success log.
