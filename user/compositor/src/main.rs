// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Run window policy and damage composition in a protected native user process.
//! Kernel authority only exposes copied IPC, physical events and checked synchronous scanout.

use anova_gui::{Compositor, Error, Reply, Request, STORAGE_PIXELS, reply_kind, transport as t};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

const SCREEN_PIXELS: usize = 800 * 600;

struct Storage {
    surfaces: [u32; STORAGE_PIXELS],
    frame: [u32; SCREEN_PIXELS],
}

struct PrivateStorage(UnsafeCell<Storage>);
// SAFETY: This executable is single-threaded, never forks, and acquires STORAGE once.
// No signal callback, kernel or other process receives a pointer into its private writable BSS.
unsafe impl Sync for PrivateStorage {}

static CLAIMED: AtomicBool = AtomicBool::new(false);
static STORAGE: PrivateStorage = PrivateStorage(UnsafeCell::new(Storage {
    surfaces: [0; STORAGE_PIXELS],
    frame: [0; SCREEN_PIXELS],
}));

fn present(core: &mut Compositor<'_>, frame: &mut [u32], width: u32) {
    if let Some(rect) = core.render(frame).unwrap() {
        let offset = rect.y as usize * width as usize + rect.x as usize;
        let scanout = t::Scanout {
            pixels: frame[offset..].as_ptr() as u64,
            x: rect.x as u32,
            y: rect.y as u32,
            width: rect.width,
            height: rect.height,
            stride: width,
            reserved: 0,
        };
        // SAFETY: Scanout is fully initialized; all pointed pixels remain readable in this
        // private frame for the complete checked kernel copy, and no pointer is retained.
        unsafe { gui::input(t::PRESENT, &scanout) }.unwrap();
    }
}

fn deliver(core: &mut Compositor<'_>, client: u64, reply: &Reply) {
    match gui::deliver(client, reply) {
        Ok(()) | Err(BROKEN_PIPE) => {}
        Err(WOULD_BLOCK) => {
            // A non-consuming client must not block the server or receive an incomplete
            // input history. Evict its bounded mailbox and require an explicit reconnect.
            // SAFETY: DROP_CLIENT interprets this argument as a scalar identity, never memory.
            match unsafe { gui::call(t::DROP_CLIENT, client, 0) } {
                Ok(_) | Err(BROKEN_PIPE) => {}
                Err(_) => panic!("GUI mailbox eviction failed"),
            }
            core.disconnect(client);
            gui::log(format_args!(
                "[GUI:BACKPRESSURE] client={client} disconnected=true\n"
            ));
        }
        Err(_) => panic!("GUI reply transport failed"),
    }
}

fn reset_old_focus(core: &mut Compositor<'_>, previous: Option<u64>, window: Option<u64>) {
    if previous != core.focused_client()
        && let Some(client) = previous
    {
        let mut reset = Reply::success(0, core.session());
        reset.kind = reply_kind::RESET;
        reset.window = window.unwrap_or(0);
        deliver(core, client, &reset);
    }
}

fn request(core: &mut Compositor<'_>, envelope: &t::Envelope) -> Reply {
    let length = envelope.length as usize;
    if !(64..=t::PACKET_BYTES).contains(&length) || !(length - 64).is_multiple_of(4) {
        return Reply::failure(0, core.session(), Error::BufferLength);
    }
    let request = match Request::from_bytes(&envelope.payload[..64]) {
        Ok(request) => request,
        Err(error) => return Reply::failure(0, core.session(), error),
    };
    let mut pixels = [0; 1008];
    let count = (length - 64) / 4;
    for (pixel, bytes) in pixels
        .iter_mut()
        .zip(envelope.payload[64..length].chunks_exact(4))
    {
        *pixel = u32::from_le_bytes(bytes.try_into().unwrap());
    }
    core.apply(envelope.client, &request, &pixels[..count])
}

fn run(test: bool, restart: bool) -> u32 {
    let session = match gui::scalar(t::BIND) {
        Ok(session) => session,
        Err(UNSUPPORTED) => return 2,
        Err(_) => panic!("GUI compositor authority denied"),
    };
    let mut display = t::DisplayInfo::default();
    // SAFETY: DisplayInfo is the exact initialized integer-only INFO output layout.
    unsafe { gui::output(t::INFO, &mut display) }.unwrap();
    assert_eq!(display.session, session);
    assert!(
        !CLAIMED.swap(true, Ordering::AcqRel),
        "GUI storage already borrowed"
    );
    // SAFETY: The atomic one-time claim and single-threaded/no-fork contract give this main
    // exclusive ownership of initialized BSS for the entire process lifetime. The two fields
    // are disjoint; the kernel only borrows framebuffer pixels synchronously during PRESENT.
    let storage = unsafe { &mut *STORAGE.0.get() };
    let mut core = Compositor::new(
        display.width,
        display.height,
        session,
        &mut storage.surfaces,
    )
    .unwrap();
    present(&mut core, &mut storage.frame, display.width);
    gui::log(format_args!(
        "[GUI:SERVER_READY] pid={} session={session} userspace=true\n",
        rt::info().unwrap().process_id
    ));
    let mut envelope = t::Envelope::default();
    let mut no_input = false;
    let mut no_input_deadline = None;
    loop {
        for _ in 0..8 {
            match gui::receive(&mut envelope) {
                Err(WOULD_BLOCK) => break,
                Err(_) => panic!("GUI server receive failed"),
                Ok(()) => {}
            }
            assert_eq!(envelope.session, session);
            match envelope.kind {
                t::CONNECTED => core.connect(envelope.client).unwrap(),
                t::DISCONNECTED => {
                    let had_surface = core.client_window(envelope.client).is_some();
                    let removed = core.disconnect(envelope.client) && had_surface;
                    gui::log(format_args!(
                        "[GUI:CLIENT_GONE] client={} surface_removed={removed}\n",
                        envelope.client
                    ));
                }
                t::REQUEST => {
                    let previous = core.focused_client();
                    let window = core.focused_window();
                    let reply = request(&mut core, &envelope);
                    reset_old_focus(&mut core, previous, window);
                    // A successful PRESENT reply follows actual CPU scanout completion.
                    present(&mut core, &mut storage.frame, display.width);
                    deliver(&mut core, envelope.client, &reply);
                }
                _ => panic!("GUI invalid kernel envelope"),
            }
        }
        for _ in 0..16 {
            let mut event = t::HardwareEvent::default();
            // SAFETY: HardwareEvent is the exact initialized 24-byte INPUT output layout.
            match unsafe { gui::output(t::INPUT, &mut event) } {
                Err(WOULD_BLOCK) => break,
                Err(UNSUPPORTED) => {
                    if !no_input {
                        rt::write(b"[GUI:NO_INPUT]\n");
                    }
                    no_input = true;
                    break;
                }
                Err(_) => panic!("GUI physical input receive failed"),
                Ok(()) => {}
            }
            if event.kind == t::KEY && event.flags & 255 == 0 {
                if test && event.value == 88 {
                    rt::write(b"[GUI:SERVER_FAULT]\n");
                    // SAFETY: This isolated test-only #UD proves kernel-owned server teardown
                    // and reconnection; the supervisor checks the actual exception status.
                    unsafe {
                        core::arch::asm!("ud2", options(noreturn));
                    }
                }
                if restart && event.value == 87 {
                    return 0;
                }
            }
            let previous = core.focused_client();
            let window = core.focused_window();
            let routed = match event.kind {
                t::RESET => core.input_reset(),
                t::KEY => core.key(event.value, (event.flags & 255) + 1, event.flags >> 8),
                t::POINTER => core.pointer(event.x, event.y, event.value, event.flags),
                _ => panic!("GUI invalid physical event kind"),
            };
            reset_old_focus(&mut core, previous, window);
            present(&mut core, &mut storage.frame, display.width);
            if let Some(routed) = routed {
                deliver(&mut core, routed.client, &routed.reply);
            }
            if event.kind == t::POINTER {
                gui::log(format_args!(
                    "[GUI:POINTER] x={} y={} buttons={}\n",
                    event.x, event.y, event.value
                ));
            }
        }
        present(&mut core, &mut storage.frame, display.width);
        if no_input && test && core.window_count() == 2 {
            let ticks = rt::info().unwrap().ticks;
            let deadline = *no_input_deadline.get_or_insert(ticks + 500);
            if ticks >= deadline {
                return 74;
            }
        }
        rt::sleep(1).unwrap();
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The loader supplies bounded immutable arguments through the native CRT.
    let mode = unsafe { rt::Args::from_raw(argc, argv) }
        .nth(1)
        .unwrap_or(b"");
    run(mode == b"test", mode == b"restart")
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[GUI:FAIL] compositor\n");
    rt::exit(101)
}
