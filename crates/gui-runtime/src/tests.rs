//! Exercise layout, raster bounds, UTF-8 editing, ABI rejection and copied RPC lifecycle.
//! The fake transport acknowledges real request layouts without invoking native syscalls.
//!
//! SPDX-License-Identifier: Apache-2.0

use super::*;
use runtime::{Runtime, Transport};
use wire::{Reply, Request, op, reply_kind};

#[derive(Default)]
struct Fake {
    pending: Option<Request>,
    inputs: usize,
    closes: usize,
    uploads: usize,
    pixels: usize,
    broken: bool,
    event: Option<Reply>,
    next_status: i32,
}

impl Transport for Fake {
    fn connect(&mut self) -> Result<u64, Error> {
        Ok(17)
    }
    fn send(&mut self, request: Request, pixels: &[u32]) -> Result<(), Error> {
        assert!(self.pending.is_none());
        if request.opcode == op::UPLOAD {
            assert_eq!(request.pixel_count as usize, pixels.len());
            assert!(pixels.len() <= 1008);
            self.uploads += 1;
            self.pixels += pixels.len();
        }
        self.pending = Some(request);
        Ok(())
    }
    fn poll(&mut self) -> Result<Reply, Error> {
        if self.broken {
            return Err(Error::Disconnected);
        }
        if let Some(event) = self.event.take() {
            return Ok(event);
        }
        if self.inputs != 0 {
            self.inputs -= 1;
            return Ok(Reply {
                kind: reply_kind::KEY,
                window: 77,
                value: 30,
                width: 1,
                ..Reply::success(0, 17)
            });
        }
        let request = self.pending.take().ok_or(Error::WouldBlock)?;
        Ok(Reply {
            window: 77,
            width: request.width,
            height: request.height,
            status: core::mem::take(&mut self.next_status),
            ..Reply::success(request.sequence, request.session)
        })
    }
    fn close(&mut self) {
        self.closes += 1;
    }
    fn pause(&mut self) {}
}

#[test]
fn server_disconnect_invalidates_the_local_generation_and_pending_events() {
    let mut runtime = Runtime::new(Fake::default());
    let handle = runtime.create(GuiWindowDesc::new(0, 0, 32, 24)).unwrap();
    runtime.transport.broken = true;
    assert_eq!(runtime.next(handle), Err(Error::Disconnected));
    assert_eq!(runtime.canvas(handle).err(), Some(Error::Stale));
    assert_eq!(runtime.destroy(handle), Err(Error::Stale));
    assert_eq!(runtime.transport.closes, 1);
}

#[test]
fn concurrent_resize_interrupts_present_until_the_geometry_event_is_consumed() {
    for reply_status in [0, wire::status::INVALID_SURFACE] {
        let mut runtime = Runtime::new(Fake::default());
        let handle = runtime.create(GuiWindowDesc::new(0, 0, 240, 160)).unwrap();
        runtime.transport.event = Some(Reply {
            kind: reply_kind::POINTER,
            window: 77,
            width: 160,
            height: 120,
            ..Reply::success(0, 17)
        });
        runtime.transport.next_status = reply_status;
        assert_eq!(runtime.present(handle), Err(Error::WouldBlock));
        assert_eq!((runtime.width, runtime.height), (240, 160));
        assert_eq!(runtime.transport.uploads, 1);
        let event = runtime.next(handle).unwrap();
        assert_eq!((event.kind, event.width, event.height), (3, 160, 120));
        assert_eq!((runtime.width, runtime.height), (160, 120));
        runtime.present(handle).unwrap();
    }
}

#[test]
fn resize_geometry_survives_event_overflow_without_new_physical_input() {
    let mut runtime = Runtime::new(Fake::default());
    let handle = runtime.create(GuiWindowDesc::new(0, 0, 240, 160)).unwrap();
    runtime.transport.event = Some(Reply {
        kind: reply_kind::POINTER,
        window: 77,
        width: 160,
        height: 120,
        value: 1,
        flags: 1,
        ..Reply::success(0, 17)
    });
    runtime.transport.inputs = 32;
    assert_eq!(runtime.present(handle), Err(Error::WouldBlock));
    assert_eq!(runtime.next(handle).unwrap().kind, 1);
    let geometry = runtime.next(handle).unwrap();
    assert_eq!(
        (geometry.kind, geometry.width, geometry.height),
        (3, 160, 120)
    );
    assert_eq!((geometry.buttons, geometry.changed), (0, 0));
    assert_eq!((runtime.width, runtime.height), (160, 120));
    assert_eq!(runtime.next(handle), Err(Error::WouldBlock));
    runtime.present(handle).unwrap();
}

#[test]
fn remote_session_or_stale_ack_retires_the_local_window() {
    for (remote_status, expected) in [
        (wire::status::SESSION, Error::Disconnected),
        (wire::status::STALE_HANDLE, Error::Stale),
    ] {
        let mut runtime = Runtime::new(Fake::default());
        let handle = runtime.create(GuiWindowDesc::new(0, 0, 32, 24)).unwrap();
        runtime.transport.next_status = remote_status;
        assert_eq!(runtime.present(handle), Err(expected));
        assert_eq!(runtime.canvas(handle).err(), Some(Error::Stale));
        assert_eq!(runtime.destroy(handle), Err(Error::Stale));
        assert_eq!(runtime.transport.closes, 1);
    }
}

#[test]
fn us_key_translation_handles_shift_caps_control_alt_repeat_and_release() {
    assert_eq!(runtime::key_text(30, 0), 'a' as u32);
    assert_eq!(runtime::key_text(30, 1), 'A' as u32);
    assert_eq!(runtime::key_text(30, 8), 'A' as u32);
    assert_eq!(runtime::key_text(30, 9), 'a' as u32);
    assert_eq!(runtime::key_text(2, 1), '!' as u32);
    assert_eq!(runtime::key_text(30, 2), 0);
    assert_eq!(runtime::key_text(30, 4), 0);
    assert_eq!(runtime::key_text(286, 0), 0);
    let mut runtime = Runtime::new(Fake::default());
    let handle = runtime.create(GuiWindowDesc::new(0, 0, 32, 24)).unwrap();
    let mut bytes = [0; 4];
    let mut length = 0;
    for state in [1, 3, 2] {
        runtime.transport.event = Some(Reply {
            kind: reply_kind::KEY,
            window: 77,
            value: 30,
            width: state,
            ..Reply::success(0, 17)
        });
        let event = runtime.next(handle).unwrap();
        assert_eq!(event.codepoint, if state == 2 { 0 } else { 97 });
        edit_text(&mut bytes, &mut length, Some(event)).unwrap();
    }
    assert_eq!(&bytes[..length as usize], b"aa");
}

#[test]
fn malformed_widget_event_shapes_and_modifiers_are_rejected() {
    let valid = GuiEvent {
        kind: 2,
        key: 30,
        codepoint: 97,
        state: 1,
        ..GuiEvent::EMPTY
    };
    assert_eq!(valid.validate(), Ok(()));
    for event in [
        GuiEvent {
            modifiers: 16,
            ..valid
        },
        GuiEvent {
            codepoint: 0xd800,
            ..valid
        },
        GuiEvent { width: 1, ..valid },
        GuiEvent { state: 2, ..valid },
        GuiEvent { kind: 1, ..valid },
        GuiEvent { kind: 3, ..valid },
    ] {
        assert_eq!(event.validate(), Err(Error::Invalid));
    }
}

#[test]
fn lifecycle_rejects_quota_double_destroy_and_reused_generation() {
    let mut runtime = Runtime::new(Fake::default());
    let desc = GuiWindowDesc::new(1, 2, 240, 160);
    let first = runtime.create(desc).unwrap();
    assert_eq!(runtime.create(desc), Err(Error::Limit));
    assert_eq!(runtime.destroy(first), Ok(()));
    assert_eq!(runtime.destroy(first), Err(Error::Stale));
    let second = runtime.create(desc).unwrap();
    assert_ne!(first, second);
    assert_eq!(runtime.check(first), Err(Error::Stale));
    assert_eq!(runtime.check(second), Ok(()));
    assert_eq!(runtime.transport.closes, 1);
}

#[test]
fn present_copies_bounded_complete_chunks_and_preserves_ordered_events() {
    let mut runtime = Runtime::new(Fake::default());
    let handle = runtime.create(GuiWindowDesc::new(0, 0, 320, 240)).unwrap();
    runtime.transport.inputs = 2;
    runtime.present(handle).unwrap();
    assert_eq!(runtime.transport.pixels, 320 * 240);
    assert_eq!(runtime.transport.uploads, 80);
    let event = runtime.next(handle).unwrap();
    assert_eq!(
        (event.kind, event.key, event.codepoint, event.state),
        (2, 30, 97, 1)
    );
    assert_eq!(runtime.next(handle), Ok(event));
    assert_eq!(runtime.next(handle), Err(Error::WouldBlock));
}

#[test]
fn local_event_overflow_discards_history_and_publishes_reset() {
    let mut runtime = Runtime::new(Fake::default());
    let handle = runtime.create(GuiWindowDesc::new(0, 0, 32, 24)).unwrap();
    runtime.transport.inputs = 33;
    runtime.present(handle).unwrap();
    assert_eq!(runtime.next(handle).unwrap().kind, 1);
    assert_eq!(runtime.next(handle), Err(Error::WouldBlock));
}

#[test]
fn invalid_window_description_does_not_connect_or_create() {
    let mut runtime = Runtime::new(Fake::default());
    let mut desc = GuiWindowDesc::new(0, 0, 321, 24);
    assert_eq!(runtime.create(desc), Err(Error::Invalid));
    desc.width = 32;
    desc.abi_version += 1;
    assert_eq!(runtime.create(desc), Err(Error::Version));
    desc.abi_version = ABI_VERSION;
    desc.reserved = 1;
    assert_eq!(runtime.create(desc), Err(Error::Invalid));
    assert_eq!(runtime.transport.closes, 0);
}

#[test]
fn raster_clips_signed_extremes_without_touching_guard_pixels() {
    let mut storage = [0xdeadbeef; 27];
    {
        let mut canvas = render::Canvas {
            pixels: &mut storage[1..26],
            width: 5,
            height: 5,
        };
        canvas.rect(
            GuiRect {
                x: -2,
                y: -2,
                width: 3,
                height: 3,
            },
            0x123456,
        );
        assert_eq!(canvas.pixels[0], 0x123456);
        assert_eq!(canvas.pixels[1], 0xdeadbeef);
        canvas.rect(
            GuiRect {
                x: i32::MIN,
                y: i32::MIN,
                width: u32::MAX,
                height: u32::MAX,
            },
            0xabcdef,
        );
        assert!(canvas.pixels.iter().all(|value| *value == 0xabcdef));
        canvas.text(i32::MAX, i32::MAX, "out of bounds", 0);
        canvas.text(i32::MIN, i32::MIN, "out of bounds", 0);
    }
    assert_eq!(storage[0], 0xdeadbeef);
    assert_eq!(storage[26], 0xdeadbeef);
}

#[test]
fn original_text_has_known_pixels_and_unicode_replacement() {
    let mut pixels = [0; 20 * 8];
    let mut canvas = render::Canvas {
        pixels: &mut pixels,
        width: 20,
        height: 8,
    };
    canvas.text(0, 0, "a☃", 0xffffff);
    assert_eq!(&canvas.pixels[0..5], &[0, 0xffffff, 0xffffff, 0xffffff, 0]);
    assert_eq!(&canvas.pixels[6..11], &[0xffffff; 5]);
    assert_eq!(canvas.pixels[20 + 6], 0xffffff);
    assert_eq!(canvas.pixels[20 + 7], 0);
}

#[test]
fn utf8_textfield_backspace_removes_whole_scalar_and_exhaustion_is_unchanged() {
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice("a☃".as_bytes());
    let mut length = 4;
    let key = GuiEvent {
        kind: 2,
        state: 1,
        codepoint: 8,
        ..GuiEvent::EMPTY
    };
    edit_text(&mut bytes, &mut length, Some(key)).unwrap();
    assert_eq!(length, 1);
    let unicode = GuiEvent {
        codepoint: '界' as u32,
        ..key
    };
    edit_text(&mut bytes, &mut length, Some(unicode)).unwrap();
    assert_eq!(&bytes[..length as usize], "a界".as_bytes());
    let before = bytes;
    assert_eq!(
        edit_text(&mut bytes[..4], &mut length, Some(unicode)),
        Err(Error::Buffer)
    );
    assert_eq!(bytes, before);
    bytes[0] = 0xff;
    assert_eq!(edit_text(&mut bytes, &mut length, None), Err(Error::Utf8));
}

#[test]
fn layout_exhaustion_and_coordinate_overflow_preserve_cursor() {
    let mut layout = GuiLayout {
        bounds: GuiRect {
            x: -2,
            y: 5,
            width: 30,
            height: 48,
        },
        cursor: 0,
        gap: 4,
        row_height: 24,
    };
    assert_eq!(
        layout.next_row().unwrap(),
        GuiRect {
            x: -2,
            y: 5,
            width: 30,
            height: 24
        }
    );
    let before = layout;
    assert_eq!(layout.next_row(), Err(Error::Limit));
    assert_eq!(layout, before);
    layout.bounds.y = i32::MAX;
    layout.bounds.height = 100;
    assert_eq!(layout.next_row(), Err(Error::Invalid));
}

#[test]
fn c_abi_layout_and_invalid_inputs_never_touch_outputs_or_native_transport() {
    assert_eq!(core::mem::offset_of!(GuiEvent, x), 16);
    assert_eq!(core::mem::offset_of!(GuiEvent, state), 40);
    assert_eq!(core::mem::offset_of!(GuiWindowDesc, width), 16);
    let mut version = GuiVersion::default();
    // SAFETY: The live version local is writable; null/oversize inputs below must
    // be rejected before dereferencing, as required by the public C contract.
    unsafe {
        assert_eq!(
            anova_gui_query_version(ABI_VERSION + 1, &mut version),
            Error::Version as i32
        );
        assert_eq!(version, GuiVersion::default());
        assert_eq!(
            anova_gui_query_version(ABI_VERSION, core::ptr::null_mut()),
            Error::Invalid as i32
        );
        assert_eq!(anova_gui_query_version(ABI_VERSION, &mut version), 0);
        assert_eq!(version.event_size, 48);
        assert_eq!(
            anova_gui_draw_text(0, 0, 0, core::ptr::null(), 1025, 0),
            Error::Limit as i32
        );
        assert_eq!(
            anova_gui_draw_text(0, 0, 0, [0xc0, 0xaf].as_ptr(), 2, 0),
            Error::Utf8 as i32
        );
        let mut length = 3;
        assert_eq!(
            anova_gui_draw_textfield(
                0,
                GuiRect::default(),
                core::ptr::null_mut(),
                2,
                &mut length,
                core::ptr::null()
            ),
            Error::Buffer as i32
        );
    }
}

#[test]
fn process_private_clipboard_is_utf8_and_atomic_on_small_output() {
    assert_eq!(
        state(|_| {
            assert_eq!(anova_gui_window_destroy(0), Error::Busy as i32);
            Ok(())
        }),
        0
    );
    clipboard_set("clip☃").unwrap();
    let mut small = [0x55; 2];
    assert_eq!(clipboard_get(&mut small), Err(Error::Buffer));
    assert_eq!(small, [0x55; 2]);
    let mut output = [0; 16];
    let count = clipboard_get(&mut output).unwrap();
    assert_eq!(&output[..count], "clip☃".as_bytes());
}
