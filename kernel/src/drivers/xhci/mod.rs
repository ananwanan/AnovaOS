// SPDX-License-Identifier: Apache-2.0
//! Bring up one PCI xHCI controller on the BSP and validate actual DMA command completions.
//! Keep the optional device independent of storage, PS/2, the compositor and boot success.

mod registers;
mod rings;

use super::pci;
use crate::{arch::x86_64::paging::EarlyMapper, serial};
use anova_platform::acpi::PmTimer;
use core::{
    cell::UnsafeCell,
    sync::atomic::{Ordering, fence},
};
use registers::{Registers, wait};
use rings::Rings;

struct Owner {
    prepared: Option<Registers>,
    active: Option<Controller>,
}

struct BootOwner(UnsafeCell<Owner>);
// SAFETY: Only the single BSP uses this handoff, at IF=0 during boot. No interrupt accesses it.
unsafe impl Sync for BootOwner {}
static OWNER: BootOwner = BootOwner(UnsafeCell::new(Owner {
    prepared: None,
    active: None,
}));

pub fn prepare(mapper: &mut EarlyMapper<'_>, timer: PmTimer) {
    let result = pci::first_xhci()
        .map_err(|_| "pci-enumeration")
        .and_then(|function| {
            function
                .map(|pci| Registers::discover(pci, mapper, timer))
                .transpose()
        });
    match result {
        Ok(Some(regs)) => {
            // SAFETY: Sole early boot writer. The post-heap phase consumes this value once.
            unsafe { (*OWNER.0.get()).prepared = Some(regs) };
        }
        Ok(None) => serial::write(format_args!("[USB:ABSENT] class=0c0330\n")),
        Err(reason) => serial::write(format_args!("[USB:REJECT] phase=pci reason={reason}\n")),
    }
}

pub fn start() {
    // SAFETY: Called once from kernel_main with interrupts disabled after DMA32 is available.
    let owner = unsafe { &mut *OWNER.0.get() };
    let Some(regs) = owner.prepared.take() else {
        return;
    };
    match Controller::start(regs) {
        Ok(controller) => owner.active = Some(controller),
        Err(reason) => serial::write(format_args!("[USB:REJECT] phase=start reason={reason}\n")),
    }
}

struct Controller {
    regs: Registers,
    rings: Rings,
}

impl Controller {
    fn start(regs: Registers) -> Result<Self, &'static str> {
        regs.reset()?;
        let rings = Rings::new(regs.scratchpads)?;
        let mut controller = Self { regs, rings };
        if let Err(reason) = controller.run() {
            let halted = regs.halt().is_ok();
            regs.pci.command((regs.pci.read(4) as u16) & !4);
            if !halted || regs.reset().is_err() {
                // A timeout is not a DMA completion. Never recycle possibly referenced frames.
                core::mem::forget(controller);
                serial::write(format_args!("[USB:QUARANTINE] controller DMA retained\n"));
            }
            return Err(reason);
        }
        Ok(controller)
    }

    fn run(&mut self) -> Result<(), &'static str> {
        let regs = self.regs;
        regs.op.write(0x38, u32::from(regs.slots));
        regs.op.address(0x30, self.rings.dcbaa());
        regs.op.address(0x18, self.rings.command_base() | 1);
        regs.interrupt.write(0, 1); // Clear pending, keep IE clear in the polled profile.
        regs.interrupt.write(8, 1);
        regs.interrupt.address(24, self.rings.dequeue());
        fence(Ordering::SeqCst);
        regs.interrupt.address(16, self.rings.erst());
        regs.pci.command(regs.pci.read(4) as u16 | 6 | 1 << 10);
        regs.op.write(0, 1);
        wait(regs.timer, "run-timeout", || regs.op.read(4) & 1 == 0)?;
        self.command(23, 0)?;
        serial::write(format_args!(
            "[USB:XHCI] noop=complete slots={} ports={} scratchpads={} irq=poll\n",
            regs.slots, regs.ports, regs.scratchpads
        ));
        #[cfg(feature = "qemu-test")]
        {
            for _ in 0..33 {
                self.command(23, 0)?;
            }
            let slot = self.command(9, 0)?;
            if slot == 0 || slot > regs.slots || self.command(10, slot)? != slot {
                return Err("slot-completion");
            }
            serial::write(format_args!(
                "[USB:RINGS] noops=34 slot={slot} disable=complete\n"
            ));
        }
        Ok(())
    }

    fn command(&mut self, kind: u8, slot: u8) -> Result<u8, &'static str> {
        let expected = self.rings.submit(kind, slot);
        let regs = self.regs;
        regs.doorbell.write(0, 0);
        let mut result = Err("missing-completion");
        wait(regs.timer, "command-timeout", || {
            if regs.op.read(4) & (1 | 1 << 2 | 1 << 12) != 0 {
                result = Err("controller-fault");
                return true;
            }
            let Some(event) = self.rings.pop() else {
                return false;
            };
            fence(Ordering::SeqCst);
            regs.interrupt.address(24, self.rings.dequeue() | 8);
            match (event.control >> 10) & 63 {
                34 => return false, // A port status change may precede the command completion.
                33 if event.pointer == expected && event.status >> 24 == 1 => {
                    result = Ok((event.control >> 24) as u8);
                }
                _ => {
                    serial::write(format_args!(
                        "[USB:COMMAND_ERROR] command={kind} control={:#x} status={:#x}\n",
                        event.control, event.status
                    ));
                    result = Err("unexpected-completion");
                }
            }
            true
        })?;
        result
    }
}
