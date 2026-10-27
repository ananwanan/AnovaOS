//! Verify wire rejection, private surface lifecycle, damage restoration and focus routing.
//!
//! These host tests exercise pure user-space policy; they do not establish native syscall
//! isolation, process cleanup, physical input delivery or real framebuffer presentation.
//!
//! SPDX-License-Identifier: Apache-2.0

use std::{vec, vec::Vec};

use crate::*;

const SESSION: u64 = 41;
const RED: u32 = 0xc43b42;
const BLUE: u32 = 0x3879c9;

fn storage() -> Vec<u32> {
    vec![0xaabbccdd; STORAGE_PIXELS]
}

fn request(opcode: u32, window: u64) -> Request {
    Request {
        window,
        ..Request::new(opcode, SESSION)
    }
}

fn create(server: &mut Compositor<'_>, client: u64, rect: Rect) -> u64 {
    let reply = server.apply(
        client,
        &Request {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            flags: CREATE_VISIBLE,
            ..request(op::CREATE, 0)
        },
        &[],
    );
    assert!(reply.is_ok(), "create failed: {reply:?}");
    reply.window
}

fn fill(server: &mut Compositor<'_>, client: u64, handle: u64, color: u32) {
    let rect = server.window_info(handle).unwrap().rect;
    let row = vec![color; rect.width as usize];
    for y in 0..rect.height {
        let reply = server.apply(
            client,
            &Request {
                y: y as i32,
                width: rect.width,
                height: 1,
                stride: rect.width,
                pixel_count: rect.width,
                ..request(op::UPLOAD, handle)
            },
            &row,
        );
        assert!(reply.is_ok(), "upload failed: {reply:?}");
    }
    assert!(
        server
            .apply(client, &request(op::PRESENT, handle), &[])
            .is_ok()
    );
}

#[test]
fn placement_observation_ignores_hidden_and_destroyed_contract_windows() {
    let mut backing = storage();
    let mut server = Compositor::new(800, 600, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    server.connect(2).unwrap();
    let rect = Rect::new(15, 250, 240, 160);
    let first = create(&mut server, 1, rect);
    let mut hidden = request(op::CREATE, 0);
    hidden.x = 270;
    hidden.y = 250;
    hidden.width = 240;
    hidden.height = 160;
    assert!(server.apply(2, &hidden, &[]).is_ok());
    assert_eq!(server.window_count(), 2);
    assert_eq!(server.visible_rectangles().collect::<Vec<_>>(), vec![rect]);
    assert!(server.apply(1, &request(op::DESTROY, first), &[]).is_ok());
    assert_eq!(server.visible_rectangles().count(), 0);
    let replacement = create(&mut server, 1, rect);
    assert_ne!(first, replacement);
    assert_eq!(server.visible_rectangles().collect::<Vec<_>>(), vec![rect]);
}

#[test]
fn wire_is_fixed_width_little_endian_and_round_trips_every_field() {
    assert_eq!(core::mem::size_of::<Request>(), 64);
    assert_eq!(core::mem::size_of::<Reply>(), 64);
    let original = Request {
        version: 0x12345678,
        opcode: 0x11223344,
        sequence: 0x0102030405060708,
        session: u64::MAX - 3,
        window: 0x1112131415161718,
        x: i32::MIN,
        y: -43,
        width: u32::MAX,
        height: 1023,
        stride: 400,
        pixel_count: 500,
        flags: 123,
        reserved: 456,
    };
    let bytes = original.to_bytes();
    assert_eq!(&bytes[..4], &[0x78, 0x56, 0x34, 0x12]);
    assert_eq!(&bytes[8..16], &[8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(Request::from_bytes(&bytes).unwrap(), original);
    assert_eq!(Request::from_bytes(&bytes[..63]), Err(Error::BufferLength));
    assert_eq!(Request::from_bytes(&[0; 65]), Err(Error::BufferLength));
    let mut reply = Reply::failure(1234, SESSION, Error::ForeignHandle);
    reply.kind = reply_kind::POINTER;
    reply.window = 123;
    reply.x = -321;
    reply.y = 43;
    reply.width = 320;
    reply.height = 240;
    reply.value = 2;
    reply.flags = 4;
    assert_eq!(Reply::from_bytes(&reply.to_bytes()).unwrap(), reply);
    let mut malformed = reply.to_bytes();
    malformed[28] = 1;
    assert_eq!(Reply::from_bytes(&malformed), Err(Error::Malformed));
    malformed[0] = VERSION as u8 + 1;
    assert_eq!(Reply::from_bytes(&malformed), Err(Error::Version));
}

#[test]
fn construction_checks_storage_screen_and_restart_session() {
    let mut backing = storage();
    assert!(matches!(
        Compositor::new(0, 80, SESSION, &mut backing),
        Err(Error::InvalidSurface)
    ));
    assert!(matches!(
        Compositor::new(100, 80, 0, &mut backing),
        Err(Error::Session)
    ));
    assert!(matches!(
        Compositor::new(100, 80, SESSION, &mut backing[..STORAGE_PIXELS - 1]),
        Err(Error::BufferTooSmall)
    ));
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    let handle = create(&mut server, 1, Rect::new(0, 0, 64, 48));
    let mut stale = request(op::MOVE, handle);
    stale.session -= 1;
    assert_eq!(server.apply(1, &stale, &[]).status, status::SESSION);
    assert_eq!(server.window_info(handle).unwrap().rect.x, 0);
}

#[test]
fn bounded_client_and_surface_quotas_recover_after_disconnect() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    assert_eq!(server.connect(0), Err(Error::UnknownClient));
    for client in 1..=4 {
        server.connect(client).unwrap();
        create(&mut server, client, Rect::new(0, 0, MAX_WIDTH, MAX_HEIGHT));
    }
    server.connect(1).unwrap();
    assert_eq!(server.connected_clients(), MAX_CLIENTS);
    assert_eq!(server.connect(5), Err(Error::Quota));
    let extra = Request {
        width: 64,
        height: 48,
        ..request(op::CREATE, 0)
    };
    assert_eq!(server.apply(1, &extra, &[]).status, status::QUOTA);
    assert!(server.disconnect(2));
    assert!(!server.disconnect(2));
    assert_eq!(server.window_count(), 3);
    server.connect(5).unwrap();
    create(&mut server, 5, Rect::new(0, 0, 64, 48));
    assert_eq!(server.window_count(), 4);
}

#[test]
fn generated_handles_reject_stale_and_foreign_owners() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    server.connect(2).unwrap();
    let first = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    assert_eq!(
        server.apply(2, &request(op::DESTROY, first), &[]).status,
        status::FOREIGN_HANDLE
    );
    assert_eq!(server.window_count(), 1);
    assert!(server.apply(1, &request(op::DESTROY, first), &[]).is_ok());
    let next = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    assert_ne!(first, next);
    assert_eq!(
        server.apply(1, &request(op::HIDE, first), &[]).status,
        status::STALE_HANDLE
    );
    assert_eq!(
        server.apply(1, &request(op::HIDE, 0), &[]).status,
        status::STALE_HANDLE
    );
    assert!(server.window_info(next).unwrap().visible);
}

#[test]
fn malformed_headers_never_mutate_live_windows() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    let handle = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    let mut bad = request(op::MOVE, handle);
    bad.version += 1;
    assert_eq!(server.apply(1, &bad, &[]).status, status::VERSION);
    bad.version = VERSION;
    bad.reserved = 1;
    assert_eq!(server.apply(1, &bad, &[]).status, status::MALFORMED);
    bad.reserved = 0;
    bad.flags = 2;
    assert_eq!(server.apply(1, &bad, &[]).status, status::MALFORMED);
    bad.flags = 0;
    bad.width = 1;
    assert_eq!(server.apply(1, &bad, &[]).status, status::MALFORMED);
    bad.width = 0;
    bad.pixel_count = 1;
    assert_eq!(server.apply(1, &bad, &[0]).status, status::BUFFER_LENGTH);
    assert_eq!(
        server.apply(99, &request(op::HIDE, handle), &[]).status,
        status::UNKNOWN_CLIENT
    );
    assert_eq!(
        server.apply(1, &request(1000, handle), &[]).status,
        status::UNKNOWN_OPCODE
    );
    assert_eq!(
        server.window_info(handle).unwrap().rect,
        Rect::new(10, 10, 64, 48)
    );
    assert_eq!(
        server
            .apply(
                1,
                &Request {
                    width: 0,
                    height: 48,
                    ..request(op::CREATE, 0)
                },
                &[]
            )
            .status,
        status::INVALID_SURFACE
    );
    assert_eq!(
        server
            .apply(
                1,
                &Request {
                    width: MAX_WIDTH + 1,
                    height: 48,
                    ..request(op::CREATE, 0)
                },
                &[]
            )
            .status,
        status::INVALID_SURFACE
    );
}

#[test]
fn malformed_upload_checks_entire_buffer_before_any_pixel_changes() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    let handle = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    fill(&mut server, 1, handle, RED);
    let upload = Request {
        x: 4,
        y: 20,
        width: 2,
        height: 2,
        stride: 3,
        pixel_count: 5,
        ..request(op::UPLOAD, handle)
    };
    assert_eq!(
        server.apply(1, &upload, &[BLUE; 4]).status,
        status::BUFFER_LENGTH
    );
    assert_eq!(
        server.apply(1, &upload, &[BLUE; 6]).status,
        status::BUFFER_LENGTH
    );
    assert_eq!(
        server
            .apply(
                1,
                &Request {
                    stride: 1,
                    ..upload
                },
                &[BLUE; 5]
            )
            .status,
        status::INVALID_SURFACE
    );
    assert_eq!(
        server
            .apply(
                1,
                &Request {
                    x: i32::MAX,
                    ..upload
                },
                &[BLUE; 5]
            )
            .status,
        status::INVALID_SURFACE
    );
    assert_eq!(
        server
            .apply(
                1,
                &Request {
                    stride: u32::MAX,
                    height: 3,
                    ..upload
                },
                &[BLUE; 5]
            )
            .status,
        status::ARITHMETIC
    );
    let mut pixels = vec![0; 160 * 120];
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[30 * 160 + 14], RED);
    assert!(
        server
            .apply(1, &upload, &[BLUE, BLUE, 0, BLUE, BLUE])
            .is_ok()
    );
    assert!(server.apply(1, &request(op::PRESENT, handle), &[]).is_ok());
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[30 * 160 + 14], BLUE);
    assert_eq!(pixels[31 * 160 + 15], BLUE);
    assert_eq!(pixels[30 * 160 + 16], RED);
}

#[test]
fn damage_preserves_undamaged_pixels_and_short_buffer_retry() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    let handle = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    fill(&mut server, 1, handle, RED);
    let mut pixels = vec![0; 160 * 120 + 1];
    pixels[160 * 120] = 0xfeedface;
    server.render(&mut pixels).unwrap();
    let upload = Request {
        x: 5,
        y: 20,
        width: 1,
        height: 1,
        stride: 1,
        pixel_count: 1,
        ..request(op::UPLOAD, handle)
    };
    assert!(server.apply(1, &upload, &[BLUE]).is_ok());
    assert_eq!(server.render(&mut pixels).unwrap(), None);
    assert_eq!(pixels[30 * 160 + 15], RED);
    let present = Request {
        x: 5,
        y: 20,
        width: 1,
        height: 1,
        ..request(op::PRESENT, handle)
    };
    assert!(server.apply(1, &present, &[]).is_ok());
    assert_eq!(server.pending_damage(), Some(Rect::new(15, 30, 1, 1)));
    assert_eq!(server.render(&mut [0; 3]), Err(Error::BufferTooSmall));
    assert_eq!(
        server.render(&mut pixels).unwrap(),
        Some(Rect::new(15, 30, 1, 1))
    );
    assert_eq!(pixels[30 * 160 + 15], BLUE);
    assert_eq!(pixels[30 * 160 + 16], RED);
    assert_eq!(pixels[160 * 120], 0xfeedface);
}

#[test]
fn overlapping_window_focus_move_hide_and_disconnect_restore_lower_layers() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    server.connect(2).unwrap();
    let red = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    let blue = create(&mut server, 2, Rect::new(30, 20, 64, 48));
    fill(&mut server, 1, red, RED);
    fill(&mut server, 2, blue, BLUE);
    let mut pixels = vec![0; 160 * 120];
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[40 * 160 + 40], BLUE);
    assert_eq!(pixels[21 * 160 + 31], ACTIVE_TITLE);
    assert_eq!(server.key(30, 1, 0).unwrap().client, 2);
    assert!(server.apply(1, &request(op::FOCUS, red), &[]).is_ok());
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[40 * 160 + 40], RED);
    assert_eq!(server.key(30, 1, 0).unwrap().client, 1);
    let moved = Request {
        x: 95,
        y: 60,
        ..request(op::MOVE, red)
    };
    assert!(server.apply(1, &moved, &[]).is_ok());
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[40 * 160 + 40], BLUE);
    assert_eq!(pixels[35 * 160 + 20], BACKGROUND);
    assert!(server.apply(1, &request(op::HIDE, red), &[]).is_ok());
    assert_eq!(server.focused_client(), Some(2));
    assert!(server.apply(1, &request(op::SHOW, red), &[]).is_ok());
    assert_eq!(server.focused_client(), Some(1));
    server.disconnect(1);
    assert_eq!(server.focused_client(), Some(2));
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[90 * 160 + 110], BACKGROUND);
    assert_eq!(pixels[40 * 160 + 40], BLUE);
}

#[test]
fn pointer_focus_capture_drag_resize_and_reset_are_consistent() {
    let mut backing = storage();
    let mut server = Compositor::new(800, 600, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    server.connect(2).unwrap();
    let red = create(&mut server, 1, Rect::new(80, 80, 240, 160));
    create(&mut server, 2, Rect::new(200, 150, 240, 160));
    assert_eq!(server.pointer(100, 110, 1, 1).unwrap().client, 1);
    assert_eq!(server.focused_client(), Some(1));
    assert_eq!(server.pointer(500, 500, 1, 0).unwrap().client, 1);
    assert_eq!(server.pointer(500, 500, 0, 1).unwrap().client, 1);
    assert_eq!(server.pointer(500, 500, 0, 0), None);
    server.pointer(100, 88, 1, 1);
    server.pointer(180, 138, 1, 0);
    server.pointer(180, 138, 0, 1);
    assert_eq!(
        server.window_info(red).unwrap().rect,
        Rect::new(160, 130, 240, 160)
    );
    server.pointer(398, 288, 1, 1);
    let resized = server.pointer(438, 258, 1, 0).unwrap();
    assert_eq!((resized.reply.width, resized.reply.height), (280, 130));
    server.pointer(438, 258, 0, 1);
    server.pointer(180, 138, 1, 1);
    assert_eq!(server.input_reset().unwrap().reply.kind, reply_kind::RESET);
    server.pointer(300, 200, 0, 0);
    assert_eq!(
        server.window_info(red).unwrap().rect,
        Rect::new(160, 130, 280, 130)
    );
    assert_eq!(server.key(30, 1, 0).unwrap().client, 1);
    assert!(server.key(30, 0, 0).is_none());
    assert!(server.key(30, 1, 16).is_none());
    assert!(server.key(400, 1, 0).is_none());
}

#[test]
fn clipped_signed_geometry_and_cursor_restore_do_not_leave_stale_pixels() {
    let mut backing = storage();
    let mut server = Compositor::new(80, 60, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    let handle = create(&mut server, 1, Rect::new(-10, -10, 64, 48));
    fill(&mut server, 1, handle, RED);
    let mut pixels = vec![0; 80 * 60];
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[20 * 80], RED);
    server.pointer(79, 59, 0, 0);
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[59 * 80 + 79], 0);
    server.input_reset();
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[59 * 80 + 79], BACKGROUND);
    let moved = Request {
        x: i32::MIN,
        y: i32::MAX,
        ..request(op::MOVE, handle)
    };
    assert!(server.apply(1, &moved, &[]).is_ok());
    server.render(&mut pixels).unwrap();
    assert!(pixels.iter().all(|pixel| *pixel == BACKGROUND));
}

#[test]
fn reclaimed_surface_pixels_are_zeroed_before_slot_reuse() {
    let mut backing = storage();
    let mut server = Compositor::new(160, 120, SESSION, &mut backing).unwrap();
    server.connect(1).unwrap();
    let handle = create(&mut server, 1, Rect::new(10, 10, 64, 48));
    fill(&mut server, 1, handle, RED);
    server.disconnect(1);
    server.connect(2).unwrap();
    let replacement = create(&mut server, 2, Rect::new(10, 10, 64, 48));
    assert_ne!(replacement, handle);
    let mut pixels = vec![0; 160 * 120];
    server.render(&mut pixels).unwrap();
    assert_eq!(pixels[30 * 160 + 30], 0);
    assert_eq!(
        server.apply(1, &request(op::PRESENT, handle), &[]).status,
        status::UNKNOWN_CLIENT
    );
}
