<!-- Record measured M8 copy reductions, stack refactoring and corrected vectored-I/O boundaries. -->
# M8 code optimization

**English** | [简体中文](../../zh-CN/validation/M8_OPTIMIZATION.md)

Recorded 2026-10-22 (Asia/Shanghai). Source baseline: `b65ac15`, on
`feat/m8-linux-static`. The [original M8 evidence](M8.md) remains historical;
this record describes the subsequent optimization and its checks.

## Changes and observable effects

| Area | Before | After |
| --- | --- | --- |
| Linux C strings | One checked usercopy/page-table validation per byte, up to 129 calls | One bounded chunk per touched page; a 128-byte single-page string plus NUL needs one call |
| argv/envp ingestion | Separate pointer/string loops and temporary arrays | One decoder, caller-owned buffers filled in place; oversized arguments return E2BIG |
| Startup stack | Build native header, decode its pointers again, then encode Linux header | Shared checked downward placement for native/Linux payload, pointers and auxv; each header encoded once |
| writev descriptors | Read descriptors individually while already writing earlier elements | Copy the entire bounded array once and validate lengths/address ranges before publishing writes |
| writev faults | A later unreadable descriptor could report failure after earlier bytes were written | Array import failure leaves the file offset/data untouched; later payload faults preserve the completed byte count |
| Empty random request | Validate a pointer and check CPU entropy even for zero bytes | Return zero without accessing user memory or the entropy source |
| mmap state | Save/copy a cursor that first-fit search never reads | Remove the unused field and its fork/load/write assignments |

The copy-call reduction is measured by the decoder's host test callback, not
inferred as a whole-system speedup. No boot-time or throughput percentage is
claimed. Reads remain limited to one page per chunk, so a NUL in the final byte
of a mapped page succeeds even when the next page is unmapped. Cross-page
strings copy another chunk only when required. UTF-8/NUL/length checks, checked
address arithmetic, BSP pinning and the actual usercopy fault fixup remain.

The debug build was also checked against the fixed 64 KiB kernel stack. The
initial refactor returned large parameter structures across an activation
closure, which exceeded that stack during exec; the final implementation fills
both buffers in place and returns only a scalar length. No heap allocation or
stack-size increase was introduced.

A stack fixture containing 16 arguments and 16 environment strings of 106 bytes
now fits the actual 4 KiB startup page. The old intermediate native header
unnecessarily consumed space. Oversized 128-byte vectors still fail; alignment,
terminators, environment bytes and AT_RANDOM placement are verified.

## Verification

- 65 host tests pass. New decoder cases cover a maximum-length single-page
  string, a page-end terminator, a needed-but-unmapped next page, valid cross-page
  reads, invalid UTF-8, missing NUL, pointer overflow and 16/17-entry vectors.
- The actual Ring 3 Linux probe adds page-end pathname open/unlink, a bad exec
  vector and E2BIG, unreadable writev arrays without offset changes, invalid
  signed lengths before I/O, partial completion before a later payload fault,
  invalid fd with no vectors and getrandom(NULL, 0, 0).
- The smoke classifier requires the following marker from the successful probe;
  a startup banner alone cannot satisfy the new boundary cases:

```text
[LINUX:BOUNDARY] page_end=true vector_fault=true writev_partial=true empty_random=true
```

- Formatting, file-purpose coverage, host/kernel Clippy with warnings denied and
  native application checks pass.
- Full QEMU verification retains all 13 success/failure classifications,
  historical user isolation, CPU/FPU, process, VFS and storage reclamation.
- Ordinary UART tests retain musl/ash launch, real pipe/redirection output,
  status 0, EOF shell restart and tmpfs loss across boots.
- All six persistence scenarios retain save/reboot byte/hash integrity, native
  execution, clean e2fsck, read-only/unknown-feature rejection and real I/O errors.

The build commands and remaining M8 limits are unchanged. Logs are under
build/logs/. No remote CI result or push is claimed. The vector-import behavior
was checked against the public Linux v6.16
[iov_iter boundary](https://github.com/torvalds/linux/blob/v6.16/lib/iov_iter.c);
the adapter is independently implemented with the existing bounded M8 profile.

The updated freestanding probe SHA-256 for this validation is
`ebb2f2764f2caafd268ba287b2e4448dd64bb2abc3d706eb1e68c834e8568f25`; the generated image manifest records it with the
explicit Linux personality. The initial M8 record retains its historical hash.
