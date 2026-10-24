// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Submit owned pixel chunks from isolated red/blue processes and consume routed physical input.
//! Controlled fault and malformed-request modes exercise server validation and process cleanup.

use anova_gui::{Reply, Request, op, reply_kind, status, transport as t};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};

struct Client {
    session: u64,
    sequence: u64,
    window: u64,
    pending: [Option<Reply>; 16],
    count: usize,
}

impl Client {
    fn request(&mut self, opcode: u32) -> Request {
        self.sequence += 1;
        let mut request = Request::new(opcode, self.session);
        request.sequence = self.sequence;
        request.window = self.window;
        request
    }

    fn rpc(&mut self, request: &Request, pixels: &[u32]) -> Reply {
        loop {
            match gui::send(request, pixels) {
                Err(WOULD_BLOCK) => rt::sleep(1).unwrap(),
                result => {
                    result.unwrap();
                    break;
                }
            }
        }
        loop {
            match gui::poll() {
                Err(WOULD_BLOCK) => rt::sleep(1).unwrap(),
                Ok(reply) if reply.sequence == request.sequence => return reply,
                Ok(reply) => {
                    assert!(
                        self.count < self.pending.len(),
                        "GUI local event queue exhausted"
                    );
                    self.pending[self.count] = Some(reply);
                    self.count += 1;
                }
                Err(_) => panic!("GUI server disappeared during request"),
            }
        }
    }

    fn next(&mut self) -> Result<Reply, i64> {
        if self.count != 0 {
            let first = self.pending[0].take().unwrap();
            self.pending.copy_within(1..self.count, 0);
            self.count -= 1;
            self.pending[self.count] = None;
            return Ok(first);
        }
        gui::poll()
    }

    fn paint(&mut self, width: u32, height: u32, color: u32) {
        let mut pixels = [0_u32; 1008];
        pixels.fill(color);
        let rows = 1008 / width;
        let mut y = 0;
        while y < height {
            let count = rows.min(height - y);
            let mut upload = self.request(op::UPLOAD);
            upload.x = 0;
            upload.y = y as i32;
            upload.width = width;
            upload.height = count;
            upload.stride = width;
            upload.pixel_count = width * count;
            assert!(
                self.rpc(&upload, &pixels[..upload.pixel_count as usize])
                    .is_ok()
            );
            y += count;
        }
        let present = self.request(op::PRESENT);
        assert!(self.rpc(&present, &[]).is_ok());
    }
}

fn marker(path: &str, bytes: &[u8]) {
    let fd = rt::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE).unwrap();
    rt::write_all(fd, bytes).unwrap();
    rt::close(fd).unwrap();
}

fn exists(path: &str) -> bool {
    if let Ok(fd) = rt::open(path, OPEN_READ) {
        rt::close(fd).unwrap();
        true
    } else {
        false
    }
}

fn hold(mode: &[u8]) -> u32 {
    gui::connect().unwrap();
    let path = match mode {
        b"hold0" => "/tmp/gui-hold0",
        b"hold1" => "/tmp/gui-hold1",
        _ => "/tmp/gui-hold2",
    };
    marker(path, b"ready");
    while !exists("/tmp/gui-release") {
        rt::sleep(1).unwrap();
    }
    gui::scalar(t::CLOSE).unwrap();
    0
}

fn boundaries(client: &mut Client, red: bool) {
    let mut version = client.request(op::CREATE);
    version.version += 1;
    assert_eq!(client.rpc(&version, &[]).status, status::VERSION);
    let mut session = client.request(op::CREATE);
    session.session += 1;
    assert_eq!(client.rpc(&session, &[]).status, status::SESSION);
    let mut invalid = client.request(op::CREATE);
    invalid.width = 0;
    invalid.height = 160;
    assert_eq!(client.rpc(&invalid, &[]).status, status::INVALID_SURFACE);
    invalid.sequence += 1;
    client.sequence = invalid.sequence;
    invalid.width = 321;
    assert_eq!(client.rpc(&invalid, &[]).status, status::INVALID_SURFACE);
    let mut stale = client.request(op::SHOW);
    stale.window = u64::MAX;
    assert_eq!(client.rpc(&stale, &[]).status, status::STALE_HANDLE);
    if !red {
        let fd = rt::open("/tmp/gui-red-window", OPEN_READ).unwrap();
        let mut bytes = [0; 8];
        assert_eq!(rt::read(fd, &mut bytes).unwrap(), bytes.len());
        rt::close(fd).unwrap();
        let mut foreign = client.request(op::SHOW);
        foreign.window = u64::from_le_bytes(bytes);
        assert_eq!(client.rpc(&foreign, &[]).status, status::FOREIGN_HANDLE);
        rt::write(b"[GUI:BLUE_BOUNDARIES] owner=true\n");
    }
}

fn mailbox() -> u32 {
    let session = gui::connect().unwrap();
    let mut request = Request::new(u32::MAX, session);
    let mut count = 0;
    'send: for sequence in 1..=40 {
        request.sequence = sequence;
        loop {
            match gui::send(&request, &[]) {
                Ok(()) => {
                    count += 1;
                    break;
                }
                Err(WOULD_BLOCK) => rt::sleep(1).unwrap(),
                Err(BROKEN_PIPE) => break 'send,
                Err(_) => panic!("GUI mailbox pressure send failed"),
            }
        }
    }
    assert!(
        count >= 17,
        "GUI mailbox evicted before its bounded capacity"
    );
    rt::sleep(2).unwrap();
    assert_eq!(gui::poll(), Err(BROKEN_PIPE));
    let mut reconnected = false;
    for _ in 0..100 {
        match gui::scalar(t::CONNECT) {
            Ok(current) => {
                assert_eq!(current, session);
                reconnected = true;
                break;
            }
            Err(BUSY | NO_SPACE) => rt::sleep(1).unwrap(),
            Err(_) => panic!("GUI mailbox reconnect failed"),
        }
    }
    assert!(reconnected, "GUI mailbox reconnect timeout");
    request.sequence = 100;
    gui::send(&request, &[]).unwrap();
    // An invalid output must leave the queued reply untouched for the next valid POLL.
    loop {
        // SAFETY: The kernel guarantees rejection of the deliberately unmapped null output.
        match unsafe { gui::call(t::POLL, 0, 64) } {
            Err(WOULD_BLOCK) => rt::sleep(1).unwrap(),
            result => {
                assert_eq!(result, Err(BAD_ADDRESS));
                break;
            }
        }
    }
    let reply = gui::poll().unwrap();
    assert_eq!(reply.sequence, 100);
    assert_eq!(reply.status, status::UNKNOWN_OPCODE);
    gui::scalar(t::CLOSE).unwrap();
    rt::write(b"[GUI:MAILBOX_BOUNDARIES] bounded=true evicted=true reconnect=true copy_failure_retained=true\n");
    0
}

fn run(red: bool, restart: bool) -> u32 {
    let session = gui::connect().unwrap();
    let mut client = Client {
        session,
        sequence: 0,
        window: 0,
        pending: [None; 16],
        count: 0,
    };
    if !restart {
        boundaries(&mut client, red);
    }
    let mut create = client.request(op::CREATE);
    create.x = if red { 80 } else { 200 };
    create.y = if red { 80 } else { 150 };
    create.width = 240;
    create.height = 160;
    create.flags = 1;
    let response = client.rpc(&create, &[]);
    assert!(response.is_ok());
    client.window = response.window;
    if red {
        marker("/tmp/gui-red-window", &client.window.to_le_bytes());
        let quota = client.request(op::CREATE);
        let mut quota = Request {
            width: 32,
            height: 24,
            ..quota
        };
        quota.window = 0;
        assert_eq!(client.rpc(&quota, &[]).status, status::QUOTA);
        let mut invalid_upload = client.request(op::UPLOAD);
        invalid_upload.width = 240;
        invalid_upload.height = 1;
        invalid_upload.stride = 239;
        invalid_upload.pixel_count = 240;
        assert!(!client.rpc(&invalid_upload, &[0; 240]).is_ok());
        rt::write(b"[GUI:RED_BOUNDARIES] bad_surface=true stale=true quota=true\n");
    }
    let color = if red { 0xc43b42 } else { 0x3879c9 };
    let mut width = 240;
    let mut height = 160;
    client.paint(width, height, color);
    let name = if red { "red" } else { "blue" };
    gui::log(format_args!(
        "[GUI:WINDOW] client={name} window={} session={session}\n",
        client.window
    ));
    if restart {
        rt::write(b"[GUI:RESTART_READY]\n");
    } else if red {
        rt::write(b"[GUI:RED_READY]\n");
        marker("/tmp/gui-red-ready", b"ready");
    } else {
        rt::write(b"[GUI:BLUE_READY]\n");
        marker("/tmp/gui-blue-ready", b"ready");
    }
    loop {
        match client.next() {
            Err(WOULD_BLOCK) => rt::sleep(1).unwrap(),
            Err(BROKEN_PIPE) => {
                gui::log(format_args!("[GUI:DISCONNECTED] client={name}\n"));
                return 0;
            }
            Err(_) => panic!("GUI client receive failed"),
            Ok(event) if event.kind == reply_kind::KEY && event.width == 1 => {
                gui::log(format_args!(
                    "[GUI:KEY] client={name} code={} pressed=true\n",
                    event.value
                ));
                if red && event.value == 45 {
                    rt::write(b"[GUI:RED_FAULT]\n");
                    // SAFETY: This deliberate #UD is confined to the independent red process;
                    // the supervisor requires the real exception reason/code from SYS_WAIT.
                    unsafe {
                        core::arch::asm!("ud2", options(noreturn));
                    }
                }
            }
            Ok(event) if event.kind == reply_kind::POINTER => {
                if event.width != width || event.height != height {
                    width = event.width;
                    height = event.height;
                    client.paint(width, height, color);
                    gui::log(format_args!(
                        "[GUI:RESIZED] client={name} width={width} height={height}\n"
                    ));
                }
            }
            Ok(_) => {}
        }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The native loader supplies bounded immutable startup arguments for this image.
    let mode = unsafe { rt::Args::from_raw(argc, argv) }
        .nth(1)
        .unwrap_or(b"");
    match mode {
        b"hold0" | b"hold1" | b"hold2" => hold(mode),
        b"reject" => {
            assert_eq!(gui::scalar(t::CONNECT), Err(NO_SPACE));
            rt::write(b"[GUI:CONNECTION_QUOTA] pressure=true\n");
            0
        }
        b"mailbox" => mailbox(),
        b"red" => run(true, false),
        b"blue" => run(false, false),
        b"restart" => run(false, true),
        _ => 2,
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[GUI:FAIL] client\n");
    rt::exit(101)
}
