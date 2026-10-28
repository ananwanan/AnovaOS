<!-- Plan the resource-dependent development desktop, graphical workflow and independent recovery acceptance. -->
# M17: development desktop

**English** | [简体中文](../../zh-CN/milestones/M17_DESKTOP.md)

**Status: M17.1 locally accepted for its bounded five-boot profile. M17.2 is locally accepted within its declared PTY/GCC profile; M17.3 now has a minimal implemented file/editor loop; its full storage-fault acceptance and M17.4–M17.5 remain pending.** This proposal follows the
[M16 foundation](M16_FOUNDATION.md). The [roadmap](../ROADMAP.md) defines ordering;
[current status](../guides/STATUS.md) records implemented capabilities and evidence.
The owner deferred M16 remote closeout on 2026-10-27 and authorized M17 development. See [M17.1 acceptance](../validation/M17.1.md) for `desktop-test`, actual input/fault/reboot evidence, budgets and remaining limits. All stage exits below still apply.

## Objective and prerequisites

Make ordinary application development possible through the desktop: edit, save,
compile, run, inspect failures, correct sources and retain work across reboot.
Use the existing guest GCC/G++/Rust/Cargo seeds. Rebuilding those compilers from
source is independent work and is not a prerequisite for this desktop.

The current [GUI runtime](M14_RUNTIME.md) proves bounded native GUI clients and an
echo/pwd terminal. That historical terminal accepts only those commands and captures at most
128 output bytes; it is not an existing graphical shell for running compilers.
M17.2 now provides the separate [real PTY terminal](M17_PTY.md); reliable graphical source editing and the all-GUI workflow remain M17.3/M17.5.

Begin with M16's declared resource, VFS and ext4 work-disk profile and preserve
the existing ext2 profile. Measure the selected desktop/build workload first.
Declare RAM, heap, task, descriptor, node, inode, file and GUI quotas before
acceptance; more QEMU RAM alone does not expand the kernel's bounded resources.
Keep the Rust compositor and desktop policy in isolated user processes.

## M17.1: supervised desktop session

Provide a supervised user-space session with application launch, window focus,
movement, close and orderly session exit. Specify compositor death, client exit,
restart limits and cleanup ownership. Preserve the independent COM1 shell.
Persistent work belongs on the work disk; session restart must not erase it.

Extend the current four-client and private-window quotas only with measured
memory/IPC bounds and failure handling. Keep executable admission explicit.
Specify any required versioned C ABI extension and equivalent C/C++/Rust behavior
before changing the runtime; do not silently change frozen records or handles.

Acceptance: launch and operate three independently implemented GUI clients together
on the declared display profile; verify focus, close/relaunch and resource reuse.
Use existing clients for this stage; terminal/editor/file-browser integration is
accepted after M17.2/M17.3 in M17.5.
Exhaust the declared client/surface quota, reject stale or foreign handles and
inject a client fault; other clients and the serial recovery shell must survive.
Restart the session and reopen persisted work without claiming automatic recovery
of surfaces or unsaved buffers that have not actually been implemented.

## M17.2: graphical PTY terminal

Connect a real graphical terminal to the selected Linux-musl shell and guest
build tools through a PTY. Carry stdin, stdout and stderr, terminal size and
foreground interruption. Implement a documented ANSI subset, bounded scrollback,
long-output handling and cancellation without freezing the GUI event loop.

Resolve admission for the native GUI terminal and Linux shell/compiler processes
explicitly. Use declared boot-module entries or an independently implemented
package/service policy; the existing native/Linux persistent-disk restrictions
do not disappear when a GUI launches a process. Preserve immutable ABI markers,
explicit personalities and GUI authority checks; never infer ABI from ELF alone.

Acceptance: enter shell commands, edit input, display distinct known stdout and
stderr fixture contents, and report a nonzero child status. A PTY normally merges
the slave's output streams; do not infer original stream identity from its bytes.
Any separate log-capture channels need their own explicit policy and tests.
Compile a selected application,
interrupt a long-running child with Ctrl+C, resize during output, and exceed the
scrollback limit without a deadlock or memory leak. Verify EOF, shell restart,
PTY/task/descriptor exhaustion recovery and a responsive independent COM1 shell.
An echo/pwd demonstration alone cannot satisfy this stage.

## M17.3: files and reliable source editing

The minimum file/editor loop is now implemented; see [M17 files](M17_FILES.md)
and [its local validation scope](../validation/M17.3.md). The complete storage-fault
exit below remains pending and is not inferred from this minimum delivery.

Provide directory navigation, file open/create/rename/delete and a source editor
with dirty-buffer state, explicit save failure and overwrite/unsaved-work checks.
Keep paths, file sizes and buffer growth bounded and diagnose exhausted storage.
Specify how the editor and terminal share the selected work-disk namespace.

Define the exact supported save policy: temporary-file creation, file fsync,
rename and parent-directory persistence where those operations are implemented
and tested. If a stronger policy is unavailable, expose the actual guarantees
and preserve a usable prior copy where the chosen policy promises one.
Neither a successful rename nor an fsync log establishes arbitrary atomic
durability or power-loss recovery for ext2 or the new ext4 profile.

Acceptance: create and modify C, C++ and Rust sources, save and reopen exact
bytes, rename them and read the same paths from the graphical terminal.
Exercise file-size, inode, disk-full, write and flush failures; report the error
without falsely marking a buffer saved. Verify the declared prior-copy policy
and exact saved contents after clean shutdown/reboot; separately label any
unexecuted abrupt-power-loss behavior.

## M17.4: observations and controlled recovery

Expose build logs, command exit/fault status and available global disk, inode,
memory, task and descriptor observations with their units and sampling scope.
Add only metrics backed by actual kernel/filesystem interfaces; unavailable
values must be explicit. Existing global fault snapshots do not establish
per-process CPU time, RSS, ownership accounting or exact resource peaks.

Provide controlled task termination and service restart with checked identity,
access rules and lifecycle cleanup. Distinguish stopping an application from
restarting the compositor or session. Preserve diagnostic evidence and user data;
the display/input admission policy must not become a pathname-based privilege.

Acceptance: observe a known build failure, inspect its retained log and child
status, stop a selected runaway test task and verify unaffected peer progress.
Restart a deliberately failed GUI service and reconnect a client with a fresh
session. Compare reported global capacities with the underlying interfaces and
verify stale task identifiers cannot terminate a different replacement task.

## M17.5: complete graphical development workflow

Use only desktop input for each declared C, C++ and Rust application:
edit → save → build → run → introduce a real source error → inspect diagnostics →
repair → rebuild → run → clean shutdown → reboot → reopen and run again.
Record sources, tool/SDK identities, command lines, output hashes and exit status.
Retain the previous usable executable after a rejected build; publication of a
new output must follow a specified failure-safe policy rather than truncating it.

Run the declared build while another GUI client receives input. Set an interaction
latency bound before execution and retain timestamped input/presentation evidence,
including violations; a screenshot alone does not prove responsiveness.
Inject a client crash and compositor loss, check isolation and explicit reconnect,
and verify serial recovery and clean shutdown without discarding saved work.
Keep the earlier runtime, storage, toolchain and GUI regression gates applicable.

Acceptance requires exact source/output retention after reboot, real compiler
success and failure, measured concurrent interaction, resource cleanup and the
declared recovery results. Record implementation, local QEMU acceptance, remote
CI and physical hardware separately; no single successful demo proves all four.

## Failure boundaries and later work

The desktop should diagnose ordinary application, build, file and service
failures while the kernel, storage and input/display paths remain usable.
Kernel panic, boot failure, broken display/input and unrecoverable storage faults
still need serial diagnostics, QEMU controls or host-assisted recovery.

After M17, a separately scoped M18 may pursue a measured compiler-source stage
and guest-only image generation. Full GCC/Rust/LLVM bootstrap, native libc/std,
SMP, kernel preemption, general GPU acceleration, USB, NVMe/AHCI and broad Linux
GUI compatibility remain independent workstreams with their own prerequisites
and acceptance; they are not implicit requirements for this development desktop.

Implemented M17.2 contract: [M17_PTY](M17_PTY.md); actual two-boot evidence: [M17.2](../validation/M17.2.md). The historical M14 echo/pwd example remains separately scoped.
