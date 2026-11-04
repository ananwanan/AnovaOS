// SPDX-License-Identifier: Apache-2.0
//! Require CPU/memory/native/Linux runtime, checked file publication and storage proof plus exits.

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Timeout,
    Panic,
    Exception,
    Exit(Option<i32>),
    Markers,
}

pub fn assess(log: &str, code: Option<i32>, timed_out: bool) -> Result<(), Failure> {
    if timed_out {
        return Err(Failure::Timeout);
    }
    if log.contains("[PANIC]") {
        return Err(Failure::Panic);
    }
    if log.contains("[EXCEPTION]") {
        return Err(Failure::Exception);
    }
    // isa-debug-exit transforms the guest value as (value << 1) | 1.
    if code != Some(33) {
        return Err(Failure::Exit(code));
    }
    if log.contains("[DISK:REJECT]") || log.contains("[BLK:QUARANTINE]") {
        return Err(Failure::Markers);
    }
    let mut remaining = log;
    for marker in [
        "[BOOT:ENTRY]",
        "[BOOT:PROTOCOL]",
        "[BOOT:MEMORY]",
        "[CPU:PAGING]",
        "[CPU:TABLES]",
        "[TRAP:BREAKPOINT]",
        "[CPU:ACPI]",
        "[CPU:IOAPIC]",
        "[CPU:APIC]",
        "[MM:FRAMES]",
        "[MM:TABLES]",
        "[MM:HEAP]",
        "[MM:STACK]",
        "[MM:TEST]",
        "[MM:ALLOC]",
        "[MM:READY]",
        "[CPU:TIMER]",
        "[TRAP:BREAKPOINT]",
        "[USER:CPU]",
        "[APP:RUST]",
        "[USER:EXIT] pid=1 status=0",
        "[APP:C]",
        "[USER:EXIT] pid=2 status=7",
        "[USER:COPYFAULT] write=false",
        "[USER:COPYFAULT] write=true",
        "[USER:LOADER]",
        "[USER:TEST]",
        "[PROC:OOM]",
        "[SCHED:TEST]",
        "[SCHED:READY]",
        "[USER:READY]",
        "[VFS:MOUNTS]",
        "[BLK:READY]",
        "[BLK:BOUNDS]",
        "[DISK:MOUNT]",
        "[INIT:START] pid=1",
        "[DISK:TEST]",
        "[FS:TEST] passed=true",
        "[SHELL:TEST]",
        "[INIT:TEST]",
        "[VFS:IO]",
        "[BLK:RESET]",
        "[VFS:READY]",
        "[BOOT:OK]",
        "[TEST:PASS]",
    ] {
        let Some(index) = remaining.find(marker) else {
            return Err(Failure::Markers);
        };
        remaining = &remaining[index + marker.len()..];
    }
    for marker in [
        "[M9:THREADS] pthread=true TLS=true mutex=true cond=true join=true shared_fd=true futex_timeout=true",
        "[M9:SHARED] fork_mapping=true futex_cross_process=true",
        "[M9:SIGNALS] frame=true sigreturn=true SSE=true EINTR=true SA_RESTART=true ppoll_mask=true",
        "[M9:POLL] nonblocking=true readiness=true timeout=true EOF=true HUP=true NVAL=true",
        "[M9:LIFECYCLE] exec_thread=true identity=true missing_interpreter=true fault_group=true exit_group=true bad_sigreturn=true",
        "[M9:TTY] PTY=true lock=true canonical=true raw=true poll=true foreground=true CtrlC=true hangup=true",
    ] {
        if log.matches(marker).count() < 3 {
            return Err(Failure::Markers);
        }
    }
    for marker in [
        "[M9:DYNAMIC] interpreter=true shared_library=true dlopen=true dynamic_TLS=true missing_library=true",
        "[M9:MISSING-LIB] Error loading shared library /lib/missing-m9.so: No such file or directory",
        "[LINUX:INTERPRETER] path=/lib/missing-m9-loader.so errno=-2",
        "[M9:OOM] shared_metadata=true fork=rolled_back exec=retained spawn=rolled_back frames_restored=true heap_restored=true",
        "[M9:RECLAIM] image=/bin/linux-threads frames_restored=true heap_restored=true tasks=0 descriptors=0",
        "[M9:RECLAIM] image=/bin/linux-dynamic frames_restored=true heap_restored=true tasks=0 descriptors=0",
        "[M9:RECLAIM] image=/bin/linux-pie frames_restored=true heap_restored=true tasks=0 descriptors=0",
        "[LINUX:BOUNDARY] page_end=true vector_fault=true writev_partial=true empty_random=true",
        "[LINUX:MEMORY] partial_oom=rolled_back overlap=retained frames_restored=true heap_restored=true",
        "[LINUX:PROBE] errno=true mmap=true TLS=true fork=true wait=true exec=true env=true CLOEXEC=true SIGPIPE=true random=true sleep=true",
        "[LINUX:MUSL] TLS=true malloc=true errno=true file=true",
        "[LINUX:BUSYBOX] applet=echo passed=true",
        "[LINUX:BUSYBOX] applet=ls passed=true",
        "[LINUX:BUSYBOX] applet=cat passed=true",
        "[LINUX:SHELL] bytes=12 redirection=true verified=true",
        "[LINUX:SHELL] bytes=5000 redirection=true verified=true",
        "M8-status-1",
        "[PCI:BLK]",
        "[BLK:BOUNDS] last_sector=ok capacity=reject overflow=reject flush=ok",
        "[DISK:MOUNT] /persist=ext2:rw",
        "[DISK:TEST] offsets=true fork=true indirect=true sparse=true unlink=true reuse=true negatives=true",
        "[BLK:RESET] acknowledged=true dma_released=true",
        "[FS:CASE] paths-copy-offset-fork-exec-cwd passed=true",
        "[FS:PUBLICATION] checked inputs; no-clobber and replacement passed",
        "[FS:CASE] pipe-block-drain-eof-broken passed=true",
        "[FS:CASE] limits-rollback-reuse passed=true",
        "[FS:CASE] tmpfs-oom-rollback passed=true",
        "[SHELL:TEST] files=verified pipeline_bytes=5000 redirection=true cwd=true",
        "[SHELL:STATUS] code=0",
        "[SHELL:STATUS] code=7",
        "[SHELL:STATUS] code=127",
        "[SHELL:STATUS] code=1",
        "[SHELL:STATUS] code=2",
        "[INIT:TEST] shell_status=0 children_reaped=true",
        "[VFS:READY] frames_restored=true heap_restored=true tmpfs=volatile",
    ] {
        if !log.contains(marker) {
            return Err(Failure::Markers);
        }
    }
    let io = log
        .lines()
        .find(|line| line.starts_with("[VFS:IO]"))
        .ok_or(Failure::Markers)?;
    for name in ["blocks", "wakes"] {
        if field(io, name)
            .and_then(|n| n.parse::<u64>().ok())
            .is_none_or(|n| n == 0)
        {
            return Err(Failure::Markers);
        }
    }
    if field(io, "open_descriptions") != Some("0") || field(io, "pipes") != Some("0") {
        return Err(Failure::Markers);
    }
    let timer = log
        .lines()
        .find(|line| line.starts_with("[CPU:TIMER]"))
        .ok_or(Failure::Markers)?;
    let ticks = field(timer, "ticks")
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or(Failure::Markers)?;
    let nanoseconds = field(timer, "monotonic_ns")
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or(Failure::Markers)?;
    if ticks < 8
        || ticks.checked_mul(10_000_000) != Some(nanoseconds)
        || !log.contains("saved_if=0 return=iretq")
        || !log.contains("saved_if=1 return=iretq")
    {
        return Err(Failure::Markers);
    }
    let tables = log
        .lines()
        .find(|line| line.starts_with("[MM:TABLES]"))
        .ok_or(Failure::Markers)?;
    let memory = log
        .lines()
        .find(|line| line.starts_with("[MM:TEST]"))
        .ok_or(Failure::Markers)?;
    let heap = log
        .lines()
        .find(|line| line.starts_with("[MM:ALLOC]"))
        .ok_or(Failure::Markers)?;
    if field(tables, "boot_table_refs") != Some("0")
        || field(tables, "NX") != Some("on")
        || field(tables, "WP") != Some("on")
        || field(tables, "direct_map") != Some("NX")
        || field(memory, "mappings") != Some("256")
        || field(memory, "invalid_free") != Some("rejected")
        || field(memory, "frame_oom") != Some("recovered")
        || field(memory, "map_oom") != Some("rolled_back")
        || field(heap, "coalesced") != Some("true")
        || field(heap, "fallible_oom") != Some("recovered")
        || field(heap, "used") != Some("0")
    {
        return Err(Failure::Markers);
    }
    let user = log
        .lines()
        .find(|line| line.starts_with("[USER:TEST]"))
        .ok_or(Failure::Markers)?;
    if field(user, "cases") != Some("11")
        || field(user, "repeated") != Some("8")
        || field(user, "pointers") != Some("rejected")
        || field(user, "syscalls") != Some("checked")
        || field(user, "faults") != Some("contained")
        || field(user, "frames_restored") != Some("true")
        || field(user, "user_irqs")
            .and_then(|v| v.parse::<u64>().ok())
            .is_none_or(|n| n == 0)
        || !log.contains("args: hello-rust AnovaOS ring3; BSS=zero x87/SSE2=ok")
        || !log.contains("args: hello-c native-v1; BSS=zero SSE2=ok")
        || !log.contains("partial_oom=rolled_back")
        || log.contains("[APP:FAIL]")
    {
        return Err(Failure::Markers);
    }
    for case in [
        "syscalls",
        "timer",
        "page-fault",
        "kernel-read",
        "kernel-write",
        "stack-guard",
        "invalid-opcode",
        "privileged",
        "bad-rsp",
        "read-only",
        "no-execute",
    ] {
        if log
            .matches(&format!("[USER:CASE] name={case} passed=true"))
            .count()
            != 1
        {
            return Err(Failure::Markers);
        }
    }
    let scheduler = log
        .lines()
        .find(|line| line.starts_with("[SCHED:TEST]"))
        .ok_or(Failure::Markers)?;
    let number = |name| {
        field(scheduler, name)
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or(Failure::Markers)
    };
    let preemptions = number("preemptions")?;
    let preempt_switches = number("preempt_switches")?;
    let spawns = number("spawns")?;
    let forks = number("forks")?;
    if preempt_switches<2 || preempt_switches>preemptions || number("switches")?<preempt_switches
        || number("sleeps")?==0 || number("waits")?==0 || number("idle_halts")?==0
        || forks<2 || number("execs")?<1 || spawns<55
        || spawns.checked_add(forks)!=Some(number("reaps")?)
        || !log.contains("[SCHED:READY] processes_reclaimed=true descriptors=0 frames_restored=true heap_restored=true")
        || !log.contains("[PROC:OOM] spawn=rolled_back fork=rolled_back exec=retained frames_restored=true heap_restored=true")
        || log.matches("[PROC:WORKER] GPR/XMM0-15/x87/MXCSR preserved").count()!=2
    {return Err(Failure::Markers);}
    for case in [
        "preemption-fpu-fault",
        "fork-private-fd",
        "fork-cpu-state",
        "exec-reset-retain-fd",
        "wait-any-orphan-repeat",
        "capacity-reuse",
    ] {
        if log
            .matches(&format!("[PROC:CASE] {case} passed=true"))
            .count()
            != 1
        {
            return Err(Failure::Markers);
        }
    }
    Ok(())
}

pub struct Fault {
    pub name: &'static str,
    pub vector: u64,
    pub error: &'static str,
    pub ist: u8,
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|word| {
        let (name, value) = word.split_once('=')?;
        (name == key).then_some(value)
    })
}

pub fn reached_fault(log: &str, code: Option<i32>, fault: Fault) -> bool {
    if code != Some(37) || log.contains("[PANIC]") || log.contains("[TEST:PASS]") {
        return false;
    }
    let injection = format!("[TEST:INJECT] {}", fault.name);
    let Some((before, after)) = log.split_once(&injection) else {
        return false;
    };
    if !before.contains("[CPU:TABLES]") {
        return false;
    }
    let Some(exception) = after.lines().find(|line| line.starts_with("[EXCEPTION]")) else {
        return false;
    };
    let values_match = field(exception, "vector").and_then(|n| n.parse().ok())
        == Some(fault.vector)
        && field(exception, "name")
            == Some(if fault.vector == 14 {
                "page-fault"
            } else {
                fault.name
            })
        && field(exception, "error") == Some(fault.error)
        && field(exception, "ist").and_then(|n| n.parse().ok()) == Some(fault.ist)
        && field(exception, "current_if") == Some("0");
    let injected_line = after.lines().next().unwrap_or_default();
    let address_matches = match fault.vector {
        14 => {
            field(injected_line, "address")
                .is_some_and(|address| field(exception, "cr2") == Some(address))
                && log.contains(match fault.error {
                    "0x3" => "[PAGE:ERROR] present=1 write=1 user=0 reserved=0 instruction=0",
                    "0x11" => "[PAGE:ERROR] present=1 write=0 user=0 reserved=0 instruction=1",
                    _ => "[PAGE:ERROR] present=0 write=1 user=0 reserved=0 instruction=0",
                })
        }
        8 => field(injected_line, "guard")
            .and_then(|g| u64::from_str_radix(g.strip_prefix("0x")?, 16).ok())
            .zip(
                field(exception, "rsp")
                    .and_then(|g| u64::from_str_radix(g.strip_prefix("0x")?, 16).ok()),
            )
            .is_some_and(|(guard, rsp)| guard.checked_add(4096) == Some(rsp)),
        13 => log.contains("[GP:ERROR] external=0 table=0 selector_index=8191"),
        _ => true,
    };
    let m3 = matches!(
        fault.name,
        "stack-guard" | "read-only" | "no-execute" | "direct-no-execute" | "unmapped"
    );
    values_match
        && address_matches
        && log.contains("[REGISTERS]")
        && (!m3 || before.contains("[MM:READY]"))
}

pub fn reached_oom(log: &str, code: Option<i32>) -> bool {
    let Some((before, after)) = log.split_once("[TEST:INJECT] oom") else {
        return false;
    };
    code == Some(35)
        && before.contains("[MM:READY]")
        && after.contains("[MM:OOM] size=33554432 align=1 used=0 capacity=16777216")
        && after.contains("[PANIC]")
        && after.contains("memory allocation of 33554432 bytes failed")
        && !log.contains("[TEST:PASS]")
        && !log.contains("[EXCEPTION]")
}

#[cfg(test)]
mod tests {
    use super::*;
    macro_rules! runtime_markers {()=>{"[M9:THREADS] pthread=true TLS=true mutex=true cond=true join=true shared_fd=true futex_timeout=true\n[M9:SHARED] fork_mapping=true futex_cross_process=true\n[M9:SIGNALS] frame=true sigreturn=true SSE=true EINTR=true SA_RESTART=true ppoll_mask=true\n[M9:POLL] nonblocking=true readiness=true timeout=true EOF=true HUP=true NVAL=true\n[M9:LIFECYCLE] exec_thread=true identity=true missing_interpreter=true fault_group=true exit_group=true bad_sigreturn=true\n[M9:TTY] PTY=true lock=true canonical=true raw=true poll=true foreground=true CtrlC=true hangup=true\n"};}
    const GOOD: &str = concat!(
        "[FS:PUBLICATION] checked inputs; no-clobber and replacement passed\n",
        runtime_markers!(),
        runtime_markers!(),
        runtime_markers!(),
        "[M9:DYNAMIC] interpreter=true shared_library=true dlopen=true dynamic_TLS=true missing_library=true\n[M9:MISSING-LIB] Error loading shared library /lib/missing-m9.so: No such file or directory\n[LINUX:INTERPRETER] path=/lib/missing-m9-loader.so errno=-2\n[M9:OOM] shared_metadata=true fork=rolled_back exec=retained spawn=rolled_back frames_restored=true heap_restored=true\n[M9:RECLAIM] image=/bin/linux-threads frames_restored=true heap_restored=true tasks=0 descriptors=0\n[M9:RECLAIM] image=/bin/linux-dynamic frames_restored=true heap_restored=true tasks=0 descriptors=0\n[M9:RECLAIM] image=/bin/linux-pie frames_restored=true heap_restored=true tasks=0 descriptors=0\n",
        "[LINUX:BOUNDARY] page_end=true vector_fault=true writev_partial=true empty_random=true\n",
        "[LINUX:MEMORY] partial_oom=rolled_back overlap=retained frames_restored=true heap_restored=true\n[LINUX:PROBE] errno=true mmap=true TLS=true fork=true wait=true exec=true env=true CLOEXEC=true SIGPIPE=true random=true sleep=true\n[LINUX:MUSL] TLS=true malloc=true errno=true file=true\n[LINUX:BUSYBOX] applet=echo passed=true\n[LINUX:BUSYBOX] applet=ls passed=true\n[LINUX:BUSYBOX] applet=cat passed=true\n[LINUX:SHELL] bytes=12 redirection=true verified=true\n[LINUX:SHELL] bytes=5000 redirection=true verified=true\nM8-status-1\n",
        "[BOOT:ENTRY]\n[BOOT:PROTOCOL]\n[BOOT:MEMORY]\n[CPU:PAGING]\n[CPU:TABLES]\n[TRAP:BREAKPOINT] saved_if=0 return=iretq\n[CPU:ACPI]\n[CPU:IOAPIC]\n[CPU:APIC]\n[MM:FRAMES]\n[MM:TABLES] boot_table_refs=0 NX=on WP=on direct_map=NX\n[MM:HEAP]\n[MM:STACK]\n[MM:TEST] mappings=256 invalid_free=rejected frame_oom=recovered map_oom=rolled_back\n[MM:ALLOC] coalesced=true fallible_oom=recovered used=0\n[MM:READY]\n[CPU:TIMER] ticks=8 monotonic_ns=80000000\n[TRAP:BREAKPOINT] saved_if=1 return=iretq\n",
        "[USER:CPU] syscall=true return=iretq\n[APP:RUST] args: hello-rust AnovaOS ring3; BSS=zero x87/SSE2=ok\n[USER:EXIT] pid=1 status=0\n[APP:C] args: hello-c native-v1; BSS=zero SSE2=ok\n[USER:EXIT] pid=2 status=7\n[USER:COPYFAULT] write=false recovered=true\n[USER:COPYFAULT] write=true recovered=true\n[USER:LOADER] partial_oom=rolled_back\n[USER:CASE] name=syscalls passed=true\n[USER:CASE] name=timer passed=true\n[USER:CASE] name=page-fault passed=true\n[USER:CASE] name=kernel-read passed=true\n[USER:CASE] name=kernel-write passed=true\n[USER:CASE] name=stack-guard passed=true\n[USER:CASE] name=invalid-opcode passed=true\n[USER:CASE] name=privileged passed=true\n[USER:CASE] name=bad-rsp passed=true\n[USER:CASE] name=read-only passed=true\n[USER:CASE] name=no-execute passed=true\n[USER:TEST] cases=11 repeated=8 pointers=rejected syscalls=checked faults=contained frames_restored=true user_irqs=1\n",
        "[PROC:OOM] spawn=rolled_back fork=rolled_back exec=retained frames_restored=true heap_restored=true\n[PROC:WORKER] GPR/XMM0-15/x87/MXCSR preserved\n[PROC:WORKER] GPR/XMM0-15/x87/MXCSR preserved\n[PROC:CASE] preemption-fpu-fault passed=true\n[PROC:CASE] fork-private-fd passed=true\n[PROC:CASE] fork-cpu-state passed=true\n[PROC:CASE] exec-reset-retain-fd passed=true\n[PROC:CASE] wait-any-orphan-repeat passed=true\n[PROC:CASE] capacity-reuse passed=true\n[SCHED:TEST] preemptions=4 preempt_switches=3 switches=8 sleeps=4 waits=16 idle_halts=3 forks=2 execs=1 spawns=55 reaps=57\n[SCHED:READY] processes_reclaimed=true descriptors=0 frames_restored=true heap_restored=true\n[USER:READY]\n[VFS:MOUNTS]\n[PCI:BLK]\n[BLK:READY]\n[BLK:BOUNDS] last_sector=ok capacity=reject overflow=reject flush=ok\n[DISK:MOUNT] /persist=ext2:rw\n[INIT:START] pid=1\n[FS:CASE] paths-copy-offset-fork-exec-cwd passed=true\n[FS:CASE] pipe-block-drain-eof-broken passed=true\n[FS:CASE] limits-rollback-reuse passed=true\n[FS:CASE] tmpfs-oom-rollback passed=true\n[DISK:TEST] offsets=true fork=true indirect=true sparse=true unlink=true reuse=true negatives=true\n[FS:TEST] passed=true\n[SHELL:STATUS] code=0\n[SHELL:STATUS] code=7\n[SHELL:STATUS] code=127\n[SHELL:STATUS] code=1\n[SHELL:STATUS] code=2\n[SHELL:TEST] files=verified pipeline_bytes=5000 redirection=true cwd=true\n[INIT:TEST] shell_status=0 children_reaped=true\n[VFS:IO] blocks=3 wakes=3 open_descriptions=0 pipes=0\n[BLK:RESET] acknowledged=true dma_released=true\n[VFS:READY] frames_restored=true heap_restored=true tmpfs=volatile\n[BOOT:OK]\n[TEST:PASS]\n",
    );

    #[test]
    fn runtime_gate_requires_all_three_program_profiles_and_real_cleanup() {
        for marker in [
            "[M9:THREADS]",
            "[M9:SIGNALS]",
            "[M9:TTY]",
            "[M9:OOM]",
            "[M9:RECLAIM] image=/bin/linux-dynamic",
            "[M9:DYNAMIC]",
        ] {
            assert_eq!(
                assess(&GOOD.replacen(marker, "[REMOVED]", 1), Some(33), false),
                Err(Failure::Markers)
            );
        }
    }

    #[test]
    fn requires_both_ordered_stages_and_debug_exit() {
        assert_eq!(assess(GOOD, Some(33), false), Ok(()));
        assert_eq!(assess(GOOD, Some(0), false), Err(Failure::Exit(Some(0))));
        assert_eq!(assess(GOOD, None, false), Err(Failure::Exit(None)));
        assert_eq!(
            assess("[BOOT:ENTRY]", Some(33), false),
            Err(Failure::Markers)
        );
        assert_eq!(
            assess(
                "[TEST:PASS][BOOT:OK][BOOT:MEMORY][BOOT:PROTOCOL][BOOT:ENTRY]",
                Some(33),
                false
            ),
            Err(Failure::Markers)
        );
    }

    #[test]
    fn rejects_panic_and_timeout_even_with_success_markers() {
        assert_eq!(
            assess(&format!("{GOOD}[PANIC]"), Some(33), false),
            Err(Failure::Panic)
        );
        assert_eq!(assess(GOOD, Some(33), true), Err(Failure::Timeout));
        assert_eq!(assess("[PANIC]", Some(35), false), Err(Failure::Panic));
        assert_eq!(
            assess(&format!("{GOOD}[EXCEPTION]"), Some(33), false),
            Err(Failure::Exception)
        );
        assert_eq!(
            assess(&GOOD.replace("ticks=8", "ticks=1"), Some(33), false),
            Err(Failure::Markers)
        );
    }

    #[test]
    fn fault_gate_requires_real_vector_ist_address_and_exit() {
        let log = "[CPU:TABLES]\n[TEST:INJECT] page-fault address=0xffffffff80025000\n[EXCEPTION] vector=14 name=page-fault error=0x2 cr2=0xffffffff80025000 current_if=0 ist=2\n[REGISTERS]\n[PAGE:ERROR] present=0 write=1 user=0 reserved=0 instruction=0\n";
        let fault = || Fault {
            name: "page-fault",
            vector: 14,
            error: "0x2",
            ist: 2,
        };
        assert!(reached_fault(log, Some(37), fault()));
        assert!(!reached_fault(log, Some(33), fault()));
        for (old, new) in [
            ("vector=14", "vector=8"),
            ("ist=2", "ist=0"),
            ("cr2=0xffffffff80025000", "cr2=0x0"),
            ("[TEST:INJECT]", "[TEST:NOTREACHED]"),
        ] {
            assert!(!reached_fault(&log.replace(old, new), Some(37), fault()));
        }
    }

    #[test]
    fn m3_gate_requires_memory_checks_and_actual_oom_panic() {
        for (old, new) in [
            ("[MM:READY]", "[MM:NOTREADY]"),
            ("boot_table_refs=0", "boot_table_refs=1"),
            ("WP=on", "WP=off"),
            ("coalesced=true", "coalesced=false"),
            ("used=0", "used=16"),
        ] {
            assert_eq!(
                assess(&GOOD.replace(old, new), Some(33), false),
                Err(Failure::Markers)
            );
        }
        let log = "[MM:READY]\n[TEST:INJECT] oom\n[MM:OOM] size=33554432 align=1 used=0 capacity=16777216\n[PANIC] memory allocation of 33554432 bytes failed";
        assert!(reached_oom(log, Some(35)));
        assert!(!reached_oom(log, Some(33)));
        assert!(!reached_oom(
            &log.replace("capacity=16777216", "capacity=2097152"),
            Some(35)
        ));
        assert!(!reached_oom(
            &log.replace("memory allocation of", "unrelated panic"),
            Some(35)
        ));
    }

    #[test]
    fn m4_gate_rejects_missing_isolation_or_reclamation_evidence() {
        for (old, new) in [
            ("[USER:READY]", "[USER:NOTREADY]"),
            ("partial_oom=rolled_back", "partial_oom=leaked"),
            ("user_irqs=1", "user_irqs=0"),
            ("frames_restored=true", "frames_restored=false"),
            (
                "name=kernel-write passed=true",
                "name=kernel-write passed=false",
            ),
            ("[USER:COPYFAULT] write=false", "[USER:NOTCOPY] write=false"),
            ("BSS=zero x87/SSE2=ok", "BSS=bad"),
        ] {
            assert_eq!(
                assess(&GOOD.replace(old, new), Some(33), false),
                Err(Failure::Markers)
            );
        }
    }

    #[test]
    fn m5_gate_rejects_unexercised_scheduling_and_leaked_processes() {
        for (old, new) in [
            ("preempt_switches=3", "preempt_switches=0"),
            ("waits=16", "waits=0"),
            ("idle_halts=3", "idle_halts=0"),
            ("reaps=57", "reaps=56"),
            ("heap_restored=true", "heap_restored=false"),
            ("fork=rolled_back", "fork=leaked"),
            ("capacity-reuse passed=true", "capacity-reuse passed=false"),
            ("[PROC:WORKER]", "[PROC:SKIPPED]"),
        ] {
            assert_eq!(
                assess(&GOOD.replace(old, new), Some(33), false),
                Err(Failure::Markers)
            );
        }
    }
    #[test]
    fn m6_gate_requires_real_file_pipe_shell_and_cleanup_evidence() {
        for (old, new) in [
            ("[FS:TEST] passed=true", "[FS:TEST] passed=false"),
            ("[FS:PUBLICATION]", "[FS:SKIPPED]"),
            ("pipeline_bytes=5000", "pipeline_bytes=0"),
            ("blocks=3", "blocks=0"),
            ("wakes=3", "wakes=0"),
            ("open_descriptions=0", "open_descriptions=1"),
            ("pipes=0", "pipes=1"),
            ("shell_status=0", "shell_status=101"),
            ("[SHELL:STATUS] code=127", "[SHELL:STATUS] code=0"),
        ] {
            assert_eq!(
                assess(&GOOD.replace(old, new), Some(33), false),
                Err(Failure::Markers)
            );
        }
    }
    #[test]
    fn m7_gate_requires_mount_disk_io_and_acknowledged_dma_release() {
        for marker in [
            "[PCI:BLK]",
            "[DISK:MOUNT]",
            "flush=ok",
            "indirect=true",
            "dma_released=true",
        ] {
            assert_eq!(
                assess(&GOOD.replace(marker, "missing"), Some(33), false),
                Err(Failure::Markers)
            );
        }
        for failure in ["[DISK:REJECT]", "[BLK:QUARANTINE]"] {
            assert_eq!(
                assess(&format!("{GOOD}{failure}"), Some(33), false),
                Err(Failure::Markers)
            );
        }
    }
}
