<!-- Define the accepted controlled IPv4/socket, user-space TLS and persistent package contracts. -->
# M10 network and package profile

M10 adds modern virtio-net, smoltcp 0.12.0 and bounded Linux IPv4 client sockets.
It also adds an actual guest package client, a local locked repository and a
controlled HTTPS acquisition path. See [validation/M10.md](validation/M10.md)
for results. M11 guest compilers are still separate work.

## Devices, time and ownership

Network access is opt-in: `cargo xtask run --network`. Ordinary run, terminal,
persistence and the original 13-case gate still have no NIC. The QEMU backend is
user-mode networking: IPv4 10.0.2.15/24, gateway 10.0.2.2 and DNS 10.0.2.3.
No host bridge, TAP interface, firewall change or privileged network setup is required.

PCI enumeration validates the modern 1af4:1041 capabilities and maps network
BARs in a separate UC/NX window from virtio-blk. The driver negotiates VERSION_1
and MAC only. RX and TX each own one coherent DMA32 page, two descriptors and
1536-byte buffers. Modern headers are 12 bytes; Ethernet frames are at most
1514 bytes. Acquire/release fences order used indices and payload ownership.
The driver validates used IDs, lengths, ring advancement and duplicate completion.
RX bytes are copied before their descriptor is recycled. Device reset is
acknowledged before DMA release; failed reset quarantines retained pages.
Shutdown stops network DMA as well as disk DMA before ACPI S5.

The scheduler polls the interface at syscall/preemption/idle boundaries using
the calibrated 100 Hz monotonic clock. It never holds user references across a
switch. External IRQs remain masked. This is bounded polled networking, with
no SMP, asynchronous IRQ networking, DHCP, IPv6, IP forwarding or physical NIC support.

The CMOS RTC is sampled twice with update-in-progress, binary/BCD, 12/24-hour
and Gregorian date checks. QEMU explicitly uses UTC. Realtime advances from
that sample using LAPIC ticks; invalid RTC data makes realtime unavailable.
CLOCK_MONOTONIC retains its previous epoch. No NTP, wall-clock adjustment API
or certificate-verification bypass is provided.

## Socket contract

Linux wire layouts and errno stay in `task/linux/sockets.rs`. The common VFS
holds opaque socket IDs in real open descriptions. `dup`, fork, shared thread
fd tables, CLOEXEC, exec and last-close follow existing descriptor ownership.
Eight public sockets are allowed. Buffers and metadata use fallible allocation.
A preallocated set of 16 transport slots also bounds TCP connections awaiting
close; closed TCP transports drain queued data/FIN for at most five seconds.
UDP close and failed creation reclaim immediately. Shutdown releases the entire stack.

The measured profile includes:

- AF_INET SOCK_STREAM/SOCK_DGRAM; optional SOCK_NONBLOCK/SOCK_CLOEXEC.
- bind to the local or wildcard IPv4 address; TCP/UDP client connect.
- read/write, sendto/recvfrom, getsockname/getpeername, shutdown,
  SO_ERROR and SO_RCVTIMEO/SO_SNDTIMEO.
- poll/ppoll, shared O_NONBLOCK and MSG_DONTWAIT; zero-progress EAGAIN,
  connect EINPROGRESS/completion, finite timeout, peer EOF and connection refusal.
- Interruptible blocked socket frames, EINTR and the existing SA_RESTART path
  for read/write/send/receive; MSG_NOSIGNAL suppresses SIGPIPE.
- Checked source/destination copies, including address-length outputs before
  consuming a datagram; IPv4 UDP payloads are limited to 1472 bytes.
  Individual read/write copies are capped at 8 KiB and may return partial progress.

TCP listen/accept, AF_UNIX, IPv6, socketpair, sendmsg/recvmsg, vectored socket I/O,
arbitrary socket options and general Linux networking applications are not accepted.
Unsupported operations fail instead of reporting synthetic success. Ordinary
file readv was added for real musl stdio CA reads; it preserves partial progress
like writev. The native-to-Linux exec bridge now splits at most 15 ASCII-space
arguments; native v1 retains its opaque argument. Use Linux ash for rich quoting.

## DNS

`dns NAME [SERVER_IPV4 [PORT]]` runs in Ring 3 and performs a real UDP DNS A query.
It checks the randomized transaction ID, source endpoint, repeated question,
response flags and bounded compressed names/record lengths. Compression cycles,
truncated responses and missing replies fail; the deadline is two seconds.
The default is QEMU's 10.0.2.3:53. The test peer answers repo.anova locally on
10.0.2.2:18082. This is an A-record resolver, with no AAAA, DNSSEC, search domains
or TCP fallback. It is independent of musl getaddrinfo, which is not claimed here.

## User-space TLS

`pkg` and `tls-test` link the unmodified, locked Mbed TLS 2.28.10 commit
`2fc8413bfcb51354c8e679141b17b3f1a5942561` into guest user ELFs. TLS stays out of
the kernel. The client requires a valid CA chain, the expected hostname and
certificate dates, and refuses missing/invalid wall time. Entropy comes from
checked getrandom/RDRAND. The accepted peer negotiates TLS 1.2. Negative tests
require the actual X.509 mismatch/not-trusted/expired flags, not just an I/O failure.

The generated `/etc/pkg-ca.pem` is a short-lived local test trust root. Its private
key remains under build/compat/tls-fixture on the host and is never packed into
initramfs or packages. `/etc/untrusted-ca.pem` is a negative fixture. Rebuilding
refreshes an expiring fixture certificate. This test root is not a public CA bundle.
The client currently connects only to QEMU's host gateway and checks repo.anova
as its TLS identity. Arbitrary Internet repositories, redirects, proxies, modern
production TLS policy and public-registry integration are outside this acceptance.

## Package format and updates

The first-party ANOVAPK1 format is explicitly little endian, at most 128 KiB,
with a 128-byte header, up to four 96-byte entry descriptors and exact payloads.
Fields declare name, integer version, x86_64 architecture, anova.native.v1 ABI,
minimum SDK dependency 1, Apache-2.0 license and a SHA-256 for each file. A LICENSE
entry is mandatory. Entry names are bounded single components; traversal,
duplicates, unsupported modes, ABI/dependency mismatches, extra/truncated bytes
and digest failures are rejected before writing persistent files.

`/repo/INDEX` locks each package archive's whole SHA-256 independently of its
internal hashes. The built-in repository carries demo versions 1 and 2, each
containing the actual native hello-c ELF, versioned text and the project license.
The installed namespace uses `/persist/pkg/NAME-a` and `NAME-b`. Files are file0
through file3, with original name/mode/hash mapping in the installed manifest.
For the demo, file0 is the executable, file1 is message.txt and file2 is LICENSE.
User data belongs under `/persist/data/NAME` and is not removed with packages.

Installation validates the entire input, writes/fsyncs the inactive slot and
manifest, then publishes a checksummed generation record. Status picks the valid
record with the highest generation. Failed staging deletes its inactive output
and leaves the previous active version. Upgrades must increase an installed
version. Removal publishes a tombstone before deleting package files; user data
remains. The fixture failure command stops after a real staged file write to
exercise rollback. `install-file PATH SHA256` accepts an explicit external lock.

This guarantees the tested successful/shutdown and ordinary failure behavior.
The existing non-journaled ext2 backend still provides no power-loss transaction
or rollback guarantee after a physical I/O failure. Dependencies are limited to
the native SDK v1 requirement; package dependency solvers, distribution packages,
signed metadata, concurrent installers and arbitrary Linux package execution are
future work. The command runs on the current single-user system.

## Commands and validation

Host prerequisites additionally include OpenSSL and Python 3. No third-party
Python modules are needed. Source acquisitions verify the exact Git commit;
Cargo.lock pins the protocol stack and its transitive registry checksums.

```sh
cargo xtask network-test
cargo xtask package-test
cargo xtask run --network --disk build/disks/work.ext2
```

In the native guest shell:

```text
dns repo.anova 10.0.2.2 18082
pkg install demo 1
pkg status demo
/persist/pkg/demo-a/file0
pkg install demo 2
pkg remove demo
```

The DNS example requires the controlled test peer. `pkg fetch demo 2 18083`
requires the HTTPS peer and image-matching test CA. network-test checks real
TCP/UDP/DNS traffic, malformed pointers, readiness, timeout, signals, fd reuse
and refusal. package-test checks offline install/upgrade/rollback/remove, actual
installed ELF execution, user data and version retention across boots, e2fsck,
strict TLS positives/negatives and HTTPS package acquisition. Existing CPU,
memory, M8/M9 runtime, terminal and persistence gates remain required.
