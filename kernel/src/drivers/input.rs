// SPDX-License-Identifier: Apache-2.0
//! Own the QEMU i8042 ports with PM-timer-bounded setup and scheduler-only byte polling.
//! IRQ1/12 remain masked. A fixed diagnostic consumer owns events/cursor until M14.3
//! supplies a user-space compositor; physical keys never become COM1 text or public ABI.
use crate::{arch::x86_64::cpu, graphics::Cursor, serial};
use anova_input::{Dispatcher, Event, KeyState, ResetReason};
use anova_platform::{PM_HZ, acpi::PmTimer, pm_elapsed};
use core::{arch::asm, cell::UnsafeCell};

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;
const BUDGET: usize = 64;
const QUEUE: usize = 64;

struct State {
    dispatcher: Dispatcher<QUEUE>,
    cursor: Option<Cursor>,
    keyboard: bool,
    pointer: bool,
    timer: PmTimer,
    last_keyboard: u32,
    last_mouse: u32,
}

impl State {
    fn expire_partial(&mut self, now: u32) {
        if (self.dispatcher.keyboard_has_partial_sequence()
            && u64::from(pm_elapsed(self.last_keyboard, now, self.timer.mask)) >= PM_HZ / 10)
            || (self.dispatcher.mouse_has_partial_packet()
                && u64::from(pm_elapsed(self.last_mouse, now, self.timer.mask)) >= PM_HZ / 10)
        {
            self.dispatcher.reset(ResetReason::DeviceError);
        }
    }
}

struct Owner(UnsafeCell<Option<State>>);
// SAFETY: Only boot init and the BSP scheduler at IF=0 on the permanent kernel
// root access this handoff. No IRQ, user trap, allocator or AP accesses it.
unsafe impl Sync for Owner {}
static OWNER: Owner = Owner(UnsafeCell::new(None));

fn read(port: u16) -> u8 {
    let value;
    // SAFETY: CPL0, sole BSP ownership of the fixed q35 i8042 status/data ports.
    unsafe {
        asm!("in al,dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}

struct Controller(PmTimer);

impl Controller {
    fn wait(&self, mut ready: impl FnMut() -> bool) -> Result<(), &'static str> {
        let start = cpu::in32(self.0.port) & self.0.mask;
        // Both budgets are bounded. The PM timer defines 100 ms; the attempt cap
        // also prevents an unresponsive/broken timer from trapping boot forever.
        for _ in 0..200_000 {
            if ready() {
                return Ok(());
            }
            if u64::from(pm_elapsed(start, cpu::in32(self.0.port), self.0.mask)) >= PM_HZ / 10 {
                break;
            }
            core::hint::spin_loop();
        }
        Err("controller-timeout")
    }

    fn write(&self, port: u16, byte: u8) -> Result<(), &'static str> {
        self.wait(|| read(STATUS) & 2 == 0)?;
        serial::out8(port, byte);
        Ok(())
    }

    fn command(&self, byte: u8) -> Result<(), &'static str> {
        self.write(STATUS, byte)
    }

    fn receive(&self, auxiliary: bool) -> Result<u8, &'static str> {
        let mut result = None;
        let mut bad = false;
        self.wait(|| {
            let status = read(STATUS);
            if status & 1 != 0 {
                let byte = read(DATA);
                if status & 0xc0 != 0 {
                    bad = true;
                    return true;
                }
                if (status & 0x20 != 0) == auxiliary {
                    result = Some(byte);
                    return true;
                }
            }
            false
        })?;
        if bad {
            return Err("controller-byte-error");
        }
        result.ok_or("controller-source-mismatch")
    }

    fn flush(&self) -> Result<(), &'static str> {
        for _ in 0..BUDGET {
            if read(STATUS) & 1 == 0 {
                return Ok(());
            }
            let _ = read(DATA);
        }
        Err("controller-drain-limit")
    }

    fn device(&self, auxiliary: bool, byte: u8) -> Result<(), &'static str> {
        for _ in 0..3 {
            if auxiliary {
                self.command(0xd4)?;
            }
            self.write(DATA, byte)?;
            match self.receive(auxiliary)? {
                0xfa => return Ok(()),
                0xfe => continue,
                _ => return Err("device-ack-rejected"),
            }
        }
        Err("device-resend-limit")
    }

    fn setup(&self) -> Result<(bool, bool), &'static str> {
        if read(STATUS) == 0xff {
            return Err("no-controller");
        }
        self.command(0xad)?;
        self.command(0xa7)?;
        self.flush()?;
        self.command(0x20)?;
        let config = self.receive(false)?;
        self.command(0xaa)?;
        if self.receive(false)? != 0x55 {
            return Err("controller-self-test");
        }
        // Explicitly disable translation and both device IRQ sources. Keep both
        // ports disabled until their interface test and ACK sequence are ready.
        self.command(0x60)?;
        self.write(DATA, (config & !0x43) | 0x30)?;
        self.command(0xab)?;
        let keyboard = self.receive(false)? == 0 && self.keyboard().is_ok();
        if !keyboard {
            self.command(0xad)?;
        }
        self.command(0xa9)?;
        let pointer = self.receive(false)? == 0 && self.mouse().is_ok();
        if !pointer {
            self.command(0xa7)?;
        }
        self.flush()?;
        Ok((keyboard, pointer))
    }

    fn keyboard(&self) -> Result<(), &'static str> {
        self.command(0xae)?;
        self.device(false, 0xf5)?;
        self.device(false, 0xf0)?;
        self.device(false, 0x01)?; // Set 1 is explicit, not firmware-dependent translation.
        self.device(false, 0xf4)
    }

    fn mouse(&self) -> Result<(), &'static str> {
        self.command(0xa8)?;
        self.device(true, 0xff)?;
        if self.receive(true)? != 0xaa || self.receive(true)? != 0 {
            return Err("unsupported-pointer-id");
        }
        self.device(true, 0xf6)?;
        self.device(true, 0xe6)?; // Standard 1:1, relative, three-byte stream.
        self.device(true, 0xf4)
    }
}

pub fn init(timer: PmTimer) {
    cpu::without_interrupts(|| {
        let controller = Controller(timer);
        let (keyboard, pointer) = match controller.setup() {
            Ok(devices) => devices,
            Err(reason) => {
                serial::write(format_args!(
                    "[INPUT:FALLBACK] keyboard=false pointer=false reason={reason}; COM1 available\n"
                ));
                return;
            }
        };
        let cursor = Cursor::new().ok();
        let (width, height) = cursor.as_ref().map_or((800, 600), Cursor::dimensions);
        let mut dispatcher =
            Dispatcher::new(width, height).expect("Valid display input dimensions");
        #[cfg(feature = "input-test")]
        let (keyboard, pointer) = {
            assert!(keyboard && pointer, "Test input devices absent");
            self_test(&mut dispatcher);
            // Exercise a real controller stop/restart in a boot with no client
            // or injected user input yet. This is not a physical hotplug claim.
            controller
                .command(0xad)
                .expect("Test keyboard port disable");
            controller.command(0xa7).expect("Test mouse port disable");
            dispatcher.reset(ResetReason::DeviceError);
            let (keyboard, pointer) = controller.setup().expect("Test input reinitialization");
            assert!(keyboard && pointer, "Input devices did not reconnect");
            serial::write(format_args!(
                "[INPUT:SELFTEST] overflow=true malformed=true client_reconnect=true device_reconnect=true\n"
            ));
            (keyboard, pointer)
        };
        while dispatcher.pop_event().is_some() {}
        if !keyboard && !pointer {
            serial::write(format_args!(
                "[INPUT:FALLBACK] keyboard=false pointer=false reason=no-devices; COM1 available\n"
            ));
        } else {
            serial::write(format_args!(
                "[INPUT:READY] keyboard={keyboard} pointer={pointer} focus=diagnostic queue={QUEUE} polling=true cursor={}\n",
                cursor.is_some()
            ));
        }
        let now = cpu::in32(timer.port) & timer.mask;
        let state = State {
            dispatcher,
            cursor,
            keyboard,
            pointer,
            timer,
            last_keyboard: now,
            last_mouse: now,
        };
        #[cfg(feature = "input-test")]
        let mut state = state;
        #[cfg(feature = "input-test")]
        self_test_timeout(&mut state);
        // SAFETY: This sole boot writer runs before task startup, at IF=0. The
        // scheduler later borrows it only on the same BSP/permanent kernel root.
        unsafe {
            *OWNER.0.get() = Some(state);
        }
    });
}

pub fn poll() {
    assert_eq!(cpu::flags() & (1 << 9), 0, "Input polling requires IF=0");
    // SAFETY: Sole BSP scheduler caller at IF=0 on its kernel root. No callbacks
    // reenter poll, no IRQ references this owner, and no borrow survives return.
    let Some(state) = (unsafe { &mut *OWNER.0.get() }) else {
        return;
    };
    // Expire before consuming a late continuation, independently per source:
    // traffic on the other port must not keep a truncated packet/prefix alive.
    state.expire_partial(cpu::in32(state.timer.port) & state.timer.mask);
    for _ in 0..BUDGET {
        let status = read(STATUS);
        if status & 1 == 0 {
            break;
        }
        let byte = read(DATA);
        if status & 0xc0 != 0 {
            state.dispatcher.reset(ResetReason::DeviceError);
        } else if status & 0x20 != 0 {
            if state.pointer {
                state.last_mouse = cpu::in32(state.timer.port) & state.timer.mask;
                state.dispatcher.feed_mouse(byte);
            }
        } else if state.keyboard {
            state.last_keyboard = cpu::in32(state.timer.port) & state.timer.mask;
            state.dispatcher.feed_keyboard(byte);
        }
    }
    for _ in 0..QUEUE {
        let Some(event) = state.dispatcher.pop_event() else {
            break;
        };
        match event {
            Event::Key(key) => serial::write(format_args!(
                "[INPUT:KEY] code={} pressed={} repeat={} modifiers={}\n",
                key.key.number(),
                key.state != KeyState::Released,
                key.state == KeyState::Repeat,
                key.modifiers.bits()
            )),
            Event::Pointer(event) => {
                if let Some(cursor) = &mut state.cursor {
                    cursor.show(event.x as u32, event.y as u32);
                }
                serial::write(format_args!(
                    "[INPUT:POINTER] x={} y={} buttons={} dx={} dy={} changed={} overflow={}\n",
                    event.x,
                    event.y,
                    event.buttons.bits(),
                    event.dx,
                    event.dy,
                    event.changed.bits(),
                    event.overflow
                ));
            }
            Event::Reset(reason) => {
                if let Some(cursor) = &mut state.cursor {
                    cursor.hide();
                }
                serial::write(format_args!("[INPUT:RESET] reason={reason:?}\n"));
            }
        }
    }
}

#[cfg(feature = "input-test")]
fn self_test(dispatcher: &mut Dispatcher<QUEUE>) {
    for _ in 0..QUEUE + 1 {
        dispatcher.feed_keyboard(0x1e);
    }
    assert_eq!(
        dispatcher.pop_event(),
        Some(Event::Reset(ResetReason::QueueOverflow))
    );
    dispatcher.reset(ResetReason::Explicit);
    while dispatcher.pop_event().is_some() {}
    dispatcher.feed_mouse(0); // Invalid packet header must not cause a false movement.
    assert_eq!(
        dispatcher.pop_event(),
        Some(Event::Reset(ResetReason::MalformedInput))
    );
    dispatcher.disconnect();
    dispatcher.feed_keyboard(0x1e);
    assert_eq!(
        dispatcher.pop_event(),
        Some(Event::Reset(ResetReason::ClientDisconnected))
    );
    assert_eq!(dispatcher.pop_event(), None);
    dispatcher.connect();
    assert_eq!(
        dispatcher.pop_event(),
        Some(Event::Reset(ResetReason::ClientConnected))
    );
    dispatcher.feed_keyboard(0x1e);
    assert!(
        matches!(dispatcher.pop_event(), Some(Event::Key(key)) if key.state == KeyState::Pressed)
    );
    dispatcher.reset(ResetReason::Explicit);
}

#[cfg(feature = "input-test")]
fn self_test_timeout(state: &mut State) {
    // Exercise the production expiry branch with independent device timestamps,
    // including a counter wrap. No fabricated device event is presented as QMP input.
    let timeout = (PM_HZ / 10) as u32;
    let now = timeout / 2;
    state.last_keyboard = now.wrapping_sub(timeout) & state.timer.mask;
    state.last_mouse = now;
    state.dispatcher.feed_keyboard(0xe0);
    state.expire_partial(now);
    assert_eq!(
        state.dispatcher.pop_event(),
        Some(Event::Reset(ResetReason::DeviceError))
    );
    assert!(!state.dispatcher.has_partial());
    state.last_keyboard = now;
    state.last_mouse = now.wrapping_sub(timeout) & state.timer.mask;
    state.dispatcher.feed_mouse(0x08);
    state.dispatcher.feed_keyboard(0x1e);
    state.expire_partial(now);
    assert_eq!(
        state.dispatcher.pop_event(),
        Some(Event::Reset(ResetReason::DeviceError))
    );
    assert!(!state.dispatcher.has_partial());
    state.dispatcher.feed_keyboard(0x1e);
    assert!(
        matches!(state.dispatcher.pop_event(), Some(Event::Key(key)) if key.state == KeyState::Pressed)
    );
    state.dispatcher.reset(ResetReason::Explicit);
    while state.dispatcher.pop_event().is_some() {}
    state.last_keyboard = cpu::in32(state.timer.port) & state.timer.mask;
    state.last_mouse = state.last_keyboard;
    serial::write(format_args!(
        "[INPUT:TIMEOUT-TEST] sources=independent prefeed=true wrap=true\n"
    ));
}
