// SPDX-License-Identifier: Apache-2.0
//! Stable Rust uses explicit assembly entries, not the unstable x86-interrupt ABI.
//! All gates clear IF. Only the breakpoint and timer/spurious paths return.
use super::{apic, cpu, paging::EarlyMapper};
use crate::serial;
use anova_platform::VirtAddr;
use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

global_asm!(include_str!("interrupts.S"));

const STACK_BYTES: usize = 16 * 1024;
#[repr(C, align(4096))]
struct GuardedStack {
    guard: [u8; 4096],
    data: [u8; STACK_BYTES],
}
impl GuardedStack {
    const fn new() -> Self {
        Self {
            guard: [0; 4096],
            data: [0; STACK_BYTES],
        }
    }
}

#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap: u16,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Gate {
    low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    middle: u16,
    high: u32,
    reserved: u32,
}
impl Gate {
    const fn empty() -> Self {
        Self {
            low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            middle: 0,
            high: 0,
            reserved: 0,
        }
    }
    fn new(address: u64, ist: u8) -> Self {
        Self {
            low: address as u16,
            selector: 8,
            ist,
            attributes: 0x8e,
            middle: (address >> 16) as u16,
            high: (address >> 32) as u32,
            reserved: 0,
        }
    }
}

struct Tables {
    gdt: [u64; 5],
    tss: Tss,
    idt: [Gate; 256],
}
struct Storage(UnsafeCell<Tables>);
// SAFETY: Only init's one-time BSP claim writes the tables with IF=0. The CPU
// then reads them for the kernel lifetime. Test-only edits also require IF=0.
unsafe impl Sync for Storage {}
static TABLES: Storage = Storage(UnsafeCell::new(Tables {
    gdt: [0; 5],
    tss: Tss {
        reserved0: 0,
        rsp: [0; 3],
        reserved1: 0,
        ist: [0; 7],
        reserved2: 0,
        reserved3: 0,
        iomap: size_of::<Tss>() as u16,
    },
    idt: [Gate::empty(); 256],
}));
struct StackStorage(UnsafeCell<[GuardedStack; 3]>);
// SAFETY: Only the BSP CPU uses these retained stacks after init removes their
// guard pages. Rust never forms references spanning the unmapped guard pages.
unsafe impl Sync for StackStorage {}
static STACKS: StackStorage = StackStorage(UnsafeCell::new([
    GuardedStack::new(),
    GuardedStack::new(),
    GuardedStack::new(),
]));
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static BREAKPOINT_SEEN: AtomicBool = AtomicBool::new(false);

#[repr(C, packed)]
struct Descriptor {
    limit: u16,
    base: u64,
}

unsafe extern "C" {
    static anova_isr_table: [u64; 256];
}

pub fn init(mapper: &mut EarlyMapper<'_>) {
    assert_eq!(cpu::flags() & (1 << 9), 0, "Trap setup requires IF=0");
    assert!(
        !INITIALIZED.swap(true, Ordering::AcqRel),
        "Trap tables already installed"
    );
    // SAFETY: The atomic one-time claim gives the BSP unique access with IF=0.
    // No table reference escapes; after loading, only the CPU accesses the TSS.
    let tables = unsafe { &mut *TABLES.0.get() };
    let mut tops = [0; 3];
    for (index, top) in tops.iter_mut().enumerate() {
        // SAFETY: Only calculate each retained stack's aligned guard address.
        let base = unsafe { ptr::addr_of!((*STACKS.0.get())[index].guard) as u64 };
        mapper.guard(VirtAddr(base));
        *top = base + 4096 + STACK_BYTES as u64;
        serial::write(format_args!(
            "[CPU:IST] index={} guard={base:#x} top={:#x}\n",
            index + 1,
            *top
        ));
    }
    tables.tss.rsp = [tops[1], 0, 0];
    tables.tss.ist = [tops[0], tops[1], tops[2], 0, 0, 0, 0];
    let tss = ptr::addr_of!(tables.tss) as u64;
    tables.gdt = [
        0,
        0x00af_9a00_0000_ffff,
        0x00cf_9200_0000_ffff,
        103 | ((tss & 0xff_ffff) << 16) | (0x89 << 40) | (((tss >> 24) & 0xff) << 56),
        tss >> 32,
    ];
    for (vector, gate) in tables.idt.iter_mut().enumerate() {
        // SAFETY: Assembly exports a fixed 256-element address table; every
        // entry is an executable, permanent ISR in the kernel text segment.
        let address = unsafe { anova_isr_table[vector] };
        let ist = match vector {
            8 => 1,
            6 | 13 | 14 => 2,
            2 | 18 => 3,
            _ => 0,
        };
        *gate = Gate::new(address, ist);
    }
    let gdt = Descriptor {
        limit: (size_of_val(&tables.gdt) - 1) as u16,
        base: ptr::addr_of!(tables.gdt) as u64,
    };
    let idt = Descriptor {
        limit: (size_of_val(&tables.idt) - 1) as u16,
        base: ptr::addr_of!(tables.idt) as u64,
    };
    // SAFETY: Both descriptors point to retained tables with validated layouts.
    // Reload CS via a 64-bit far return, SS/DS/ES via the writable data selector,
    // and TR via the available 64-bit TSS descriptor; CPL remains zero.
    unsafe {
        asm!(
            "lgdt [{gdt}]", "push 8", "lea rax, [rip + 2f]", "push rax", "retfq", "2:",
            "mov ax, 16", "mov ss, ax", "mov ds, ax", "mov es, ax",
            "mov ax, 24", "ltr ax", "lidt [{idt}]",
            gdt = in(reg) &gdt, idt = in(reg) &idt, out("rax") _,
        );
    }
    serial::write(format_args!(
        "[CPU:TABLES] GDT TSS IDT ready; IF=0; three guarded IST stacks\n"
    ));
}

/// Hardware and assembly layout: 15 GPRs, vector/error, five 64-bit CPU words.
#[repr(C)]
pub struct TrapFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rdi: u64,
    rsi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    vector: u64,
    error: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

fn on_stack(frame: &TrapFrame, index: usize) -> bool {
    // SAFETY: Only compute field addresses through a raw pointer; never read
    // guard contents or create a reference spanning the unmapped guard pages.
    let start = unsafe { ptr::addr_of!((*STACKS.0.get())[index].data) as u64 };
    let address = frame as *const TrapFrame as u64;
    address >= start && address + size_of::<TrapFrame>() as u64 <= start + STACK_BYTES as u64
}

#[unsafe(no_mangle)]
extern "C" fn anova_trap_dispatch(frame: &TrapFrame) {
    match frame.vector {
        0x40 => {
            apic::timer_irq();
            return;
        }
        0xff => return, // APIC spurious interrupts do not enter the ISR: no EOI.
        3 => {
            BREAKPOINT_SEEN.store(true, Ordering::Relaxed);
            serial::emergency(format_args!(
                "[TRAP:BREAKPOINT] rip={:#x} saved_if={} return=iretq\n",
                frame.rip,
                (frame.rflags >> 9) & 1
            ));
            return;
        }
        _ => {}
    }
    let cr2: u64;
    // SAFETY: CPL0; capture the fault address before diagnostic code can fault.
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }
    let name = match frame.vector {
        0 => "divide-error",
        6 => "invalid-opcode",
        8 => "double-fault",
        13 => "general-protection",
        14 => "page-fault",
        _ => "unexpected-vector",
    };
    serial::emergency(format_args!(
        "\n[EXCEPTION] vector={} name={name} error={:#x} rip={:#x} cs={:#x} rflags={:#x} rsp={:#x} ss={:#x} cr2={cr2:#x} current_if={} ist={}\n",
        frame.vector,
        frame.error,
        frame.rip,
        frame.cs,
        frame.rflags,
        frame.rsp,
        frame.ss,
        (cpu::flags() >> 9) & 1,
        if on_stack(frame, 0) {
            1
        } else if on_stack(frame, 1) {
            2
        } else if on_stack(frame, 2) {
            3
        } else {
            0
        }
    ));
    serial::emergency(format_args!(
        "[REGISTERS] rax={:#x} rbx={:#x} rcx={:#x} rdx={:#x} rsi={:#x} rdi={:#x} rbp={:#x}\n[REGISTERS] r8={:#x} r9={:#x} r10={:#x} r11={:#x} r12={:#x} r13={:#x} r14={:#x} r15={:#x}\n",
        frame.rax,
        frame.rbx,
        frame.rcx,
        frame.rdx,
        frame.rsi,
        frame.rdi,
        frame.rbp,
        frame.r8,
        frame.r9,
        frame.r10,
        frame.r11,
        frame.r12,
        frame.r13,
        frame.r14,
        frame.r15
    ));
    if frame.vector == 14 {
        serial::emergency(format_args!(
            "[PAGE:ERROR] present={} write={} user={} reserved={} instruction={}\n",
            frame.error & 1,
            (frame.error >> 1) & 1,
            (frame.error >> 2) & 1,
            (frame.error >> 3) & 1,
            (frame.error >> 4) & 1
        ));
    }
    if frame.vector == 13 {
        serial::emergency(format_args!(
            "[GP:ERROR] external={} table={} selector_index={}\n",
            frame.error & 1,
            (frame.error >> 1) & 3,
            frame.error >> 3
        ));
    }
    #[cfg(feature = "qemu-test")]
    serial::out8(0xf4, 0x12); // Host 37: a real exception is always a smoke failure.
    crate::halt()
}

pub fn breakpoint_test() {
    let value: u64;
    // SAFETY: Vector 3 is installed and returns; this tests its real IRET frame
    // and GPR preservation without changing the current privilege level.
    unsafe {
        asm!("int3", inout("rax") 0x1234_5678_9abc_def0_u64 => value);
    }
    assert!(
        BREAKPOINT_SEEN.load(Ordering::Relaxed),
        "Breakpoint handler did not run"
    );
    assert_eq!(value, 0x1234_5678_9abc_def0, "Trap clobbered a GPR");
}

pub fn inject_fault() {
    if cfg!(feature = "test-invalid-opcode") {
        serial::write(format_args!("[TEST:INJECT] invalid-opcode\n"));
        // SAFETY: Deliberate fatal instruction in a separate test image.
        unsafe {
            asm!("ud2", options(noreturn));
        }
    }
    if cfg!(feature = "test-general-protection") {
        serial::write(format_args!("[TEST:INJECT] general-protection\n"));
        // SAFETY: Deliberately load an out-of-GDT data selector; the installed
        // #GP handler runs on IST2 and terminates this separate test image.
        unsafe {
            asm!("mov ax, 0xfff8", "mov ds, ax", out("ax") _);
        }
        panic!("General protection injection returned");
    }
    if cfg!(feature = "test-page-fault") {
        let guard = guard_address();
        serial::write(format_args!(
            "[TEST:INJECT] page-fault address={guard:#x}\n"
        ));
        // SAFETY: Deliberately access a removed guard mapping via assembly,
        // avoiding a Rust null/dangling reference. #PF must terminate the guest.
        unsafe {
            asm!("mov byte ptr [{}], 0", in(reg) guard, options(nostack));
        }
        panic!("Page fault injection returned");
    }
    #[cfg(feature = "test-double-fault")]
    {
        let guard = guard_address();
        serial::write(format_args!(
            "[TEST:INJECT] double-fault guard={guard:#x}\n"
        ));
        // SAFETY: This test-only edit occurs at IF=0 on the sole CPU. Force
        // #PF to use the current stack so a real stack fault during #PF delivery
        // escalates to #DF. IST1 stays independent and present throughout.
        unsafe {
            ptr::addr_of_mut!((*TABLES.0.get()).idt[14].ist).write_volatile(0);
            asm!("mov rsp, {}", "push 0", in(reg) guard + 4096, options(noreturn));
        }
    }
}

fn guard_address() -> u64 {
    // SAFETY: Only form the address, without creating a reference to the guard.
    unsafe { ptr::addr_of!((*STACKS.0.get())[1].guard) as u64 }
}

const _: () =
    assert!(size_of::<Tss>() == 104 && size_of::<Gate>() == 16 && size_of::<TrapFrame>() == 176);
