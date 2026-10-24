//! Exercise physical scan-code decoding, packet recovery and bounded diagnostic dispatch.
//!
//! SPDX-License-Identifier: Apache-2.0

use super::*;

fn key(code: u8, extended: bool) -> PhysicalKey {
    PhysicalKey { code, extended }
}

fn packet(mouse: &mut Mouse, bytes: [u8; 3]) -> MousePacket {
    assert_eq!(mouse.feed(bytes[0]), Ok(None));
    assert_eq!(mouse.feed(bytes[1]), Ok(None));
    mouse.feed(bytes[2]).unwrap().unwrap()
}

fn send_mouse<const N: usize>(input: &mut Dispatcher<N>, bytes: [u8; 3]) {
    for byte in bytes {
        input.feed_mouse(byte);
    }
}

#[test]
fn key_press_repeat_release_and_stray_release_have_distinct_semantics() {
    let mut keyboard = Keyboard::new();
    assert_eq!(keyboard.feed(0x9e), Ok(None));
    for (byte, expected) in [
        (0x1e, KeyState::Pressed),
        (0x1e, KeyState::Repeat),
        (0x9e, KeyState::Released),
    ] {
        let event = keyboard.feed(byte).unwrap().unwrap();
        assert_eq!(event.key, key(0x1e, false));
        assert_eq!(event.state, expected);
        assert_eq!(
            keyboard.is_pressed(event.key),
            expected != KeyState::Released
        );
    }
    assert_eq!(keyboard.feed(0x9e), Ok(None));
}

#[test]
fn extended_navigation_and_keypad_are_separate_physical_keys() {
    let mut keyboard = Keyboard::new();
    keyboard.feed(0x48).unwrap();
    assert_eq!(keyboard.feed(0xe0), Ok(None));
    assert!(keyboard.has_partial_sequence());
    let event = keyboard.feed(0x48).unwrap().unwrap();
    assert_eq!(event.key, key(0x48, true));
    assert_eq!(event.key.number(), 256 + 0x48);
    keyboard.feed(0xe0).unwrap();
    assert_eq!(
        keyboard.feed(0xc8).unwrap().unwrap().state,
        KeyState::Released
    );
    assert!(keyboard.is_pressed(key(0x48, false)));
    assert!(!keyboard.is_pressed(key(0x48, true)));
}

#[test]
fn left_and_right_modifiers_retain_each_others_state() {
    let mut keyboard = Keyboard::new();
    assert!(keyboard.feed(0x2a).unwrap().unwrap().modifiers.shift());
    keyboard.feed(0x36).unwrap();
    assert!(keyboard.feed(0xaa).unwrap().unwrap().modifiers.shift());
    assert!(!keyboard.feed(0xb6).unwrap().unwrap().modifiers.shift());
    keyboard.feed(0x1d).unwrap();
    keyboard.feed(0xe0).unwrap();
    keyboard.feed(0x1d).unwrap();
    assert!(keyboard.feed(0x9d).unwrap().unwrap().modifiers.control());
    keyboard.feed(0xe0).unwrap();
    assert!(!keyboard.feed(0x9d).unwrap().unwrap().modifiers.control());
    keyboard.feed(0x38).unwrap();
    assert!(keyboard.modifiers().alt());
    assert!(!keyboard.feed(0xb8).unwrap().unwrap().modifiers.alt());
}

#[test]
fn caps_lock_toggles_only_on_a_fresh_make() {
    let mut keyboard = Keyboard::new();
    assert!(keyboard.feed(0x3a).unwrap().unwrap().modifiers.caps_lock());
    assert!(keyboard.feed(0x3a).unwrap().unwrap().modifiers.caps_lock());
    assert!(keyboard.feed(0xba).unwrap().unwrap().modifiers.caps_lock());
    assert!(!keyboard.feed(0x3a).unwrap().unwrap().modifiers.caps_lock());
    keyboard.reset();
    assert_eq!(keyboard.modifiers().bits(), 0);
}

#[test]
fn print_screen_fake_shifts_do_not_mutate_real_modifiers() {
    let mut keyboard = Keyboard::new();
    keyboard.feed(0x2a).unwrap();
    for byte in [0xe0, 0x2a, 0xe0] {
        assert_eq!(keyboard.feed(byte), Ok(None));
    }
    let event = keyboard.feed(0x37).unwrap().unwrap();
    assert_eq!(event.key, key(0x37, true));
    assert_eq!(event.state, KeyState::Pressed);
    assert!(event.modifiers.shift());
    keyboard.feed(0xe0).unwrap();
    assert_eq!(
        keyboard.feed(0xb7).unwrap().unwrap().state,
        KeyState::Released
    );
    assert_eq!(keyboard.feed(0xe0), Ok(None));
    assert_eq!(keyboard.feed(0xaa), Ok(None));
    assert!(keyboard.modifiers().shift());
    assert!(!keyboard.has_partial_sequence());
}

#[test]
fn alt_print_screen_sysrq_normalizes_to_the_same_physical_key() {
    let mut keyboard = Keyboard::new();
    keyboard.feed(0x38).unwrap();
    // QEMU brackets Alt+PrintScreen with a release/repress of the held Alt key.
    keyboard.feed(0xb8).unwrap();
    keyboard.feed(0x38).unwrap();
    let event = keyboard.feed(0x54).unwrap().unwrap();
    assert_eq!(event.key, key(0x37, true));
    assert_eq!(event.state, KeyState::Pressed);
    assert!(event.modifiers.alt());
    assert_eq!(
        keyboard.feed(0xd4).unwrap().unwrap().state,
        KeyState::Released
    );
    keyboard.feed(0xb8).unwrap();
    keyboard.feed(0x38).unwrap();
    assert!(keyboard.modifiers().alt());
}

#[test]
fn pause_sequence_emits_once_and_has_no_stuck_ctrl_or_numlock() {
    let mut keyboard = Keyboard::new();
    for byte in [0xe1, 0x1d, 0x45, 0xe1, 0x9d] {
        assert_eq!(keyboard.feed(byte), Ok(None));
        assert!(keyboard.has_partial_sequence());
    }
    let event = keyboard.feed(0xc5).unwrap().unwrap();
    assert_eq!(event.key, PhysicalKey::PAUSE);
    assert_eq!(event.state, KeyState::Pressed);
    assert!(!keyboard.is_pressed(event.key));
    assert!(!keyboard.is_pressed(key(0x45, false)));
    assert_eq!(event.modifiers.bits(), 0);
    assert!(!keyboard.has_partial_sequence());
}

#[test]
fn malformed_special_sequences_do_not_consume_later_valid_keys() {
    let mut keyboard = Keyboard::new();
    keyboard.feed(0xe1).unwrap();
    assert_eq!(keyboard.feed(0), Err(DecodeError::InvalidPauseSequence));
    assert!(!keyboard.has_partial_sequence());
    assert_eq!(
        keyboard.feed(0x1e).unwrap().unwrap().state,
        KeyState::Pressed
    );
    keyboard.feed(0xe0).unwrap();
    assert_eq!(keyboard.feed(0xe0), Err(DecodeError::InvalidKeyboardCode));
    assert!(!keyboard.has_partial_sequence());
    assert_eq!(keyboard.feed(0xfa), Err(DecodeError::InvalidKeyboardCode));
    assert_eq!(keyboard.feed(0), Err(DecodeError::InvalidKeyboardCode));
}

#[test]
fn all_supported_base_keys_round_trip_without_text_translation() {
    for code in (1..=0x53).chain(0x56..=0x58) {
        let mut keyboard = Keyboard::new();
        assert_eq!(keyboard.feed(code).unwrap().unwrap().key, key(code, false));
        assert_eq!(
            keyboard.feed(code | 0x80).unwrap().unwrap().state,
            KeyState::Released
        );
    }
}

#[test]
fn mouse_axes_use_header_signs_and_keep_full_nine_bit_range() {
    let mut mouse = Mouse::new();
    assert_eq!(packet(&mut mouse, [0x08, 255, 128]).dx, 255);
    let positive = packet(&mut mouse, [0x08, 128, 255]);
    assert_eq!((positive.dx, positive.dy), (128, 255));
    let negative = packet(&mut mouse, [0x38, 0, 0]);
    assert_eq!((negative.dx, negative.dy), (-256, -256));
    let negative = packet(&mut mouse, [0x38, 255, 128]);
    assert_eq!((negative.dx, negative.dy), (-1, -128));
}

#[test]
fn mouse_buttons_emit_changes_independently_of_motion() {
    let mut mouse = Mouse::new();
    let press = packet(&mut mouse, [0x0b, 0, 0]);
    assert_eq!(press.buttons.bits(), Buttons::LEFT | Buttons::RIGHT);
    assert_eq!(press.changed.bits(), 3);
    assert_eq!(packet(&mut mouse, [0x0b, 0, 0]).changed.bits(), 0);
    let change = packet(&mut mouse, [0x0c, 0, 0]);
    assert_eq!(change.buttons.bits(), Buttons::MIDDLE);
    assert_eq!(change.changed.bits(), 7);
}

#[test]
fn overflow_discards_both_motion_axes_but_keeps_button_changes() {
    let mut mouse = Mouse::new();
    for header in [0x49, 0x89, 0xc9] {
        let event = packet(&mut mouse, [header, 255, 255]);
        assert!(event.overflow);
        assert_eq!((event.dx, event.dy), (0, 0));
        assert_eq!(event.buttons.bits(), Buttons::LEFT);
    }
    assert!(!packet(&mut mouse, [0x08, 1, 1]).overflow);
}

#[test]
fn header_resync_does_not_mistake_data_bit_three_for_a_new_packet() {
    let mut mouse = Mouse::new();
    for invalid in [0, 1, 4, 0xf7] {
        assert_eq!(mouse.feed(invalid), Err(DecodeError::InvalidMouseHeader));
        assert!(!mouse.has_partial_packet());
    }
    let event = packet(&mut mouse, [0x08, 0x08, 0x08]);
    assert_eq!((event.dx, event.dy), (8, 8));
    mouse.feed(0x08).unwrap();
    assert!(mouse.has_partial_packet());
    mouse.reset();
    assert!(!mouse.has_partial_packet());
    assert_eq!(packet(&mut mouse, [0x08, 1, 2]).dx, 1);
}

#[test]
fn pointer_clips_zero_single_pixel_and_maximum_dimensions_safely() {
    assert_eq!(Pointer::new(0, 10), None);
    assert_eq!(Pointer::new(10, 0), None);
    assert_eq!(Pointer::new(u32::MAX, 10), None);
    let mut single = Pointer::new(1, 1).unwrap();
    single.move_by(i16::MAX, i16::MIN);
    assert_eq!((single.x, single.y), (0, 0));
    let mut maximum = Pointer::new(i32::MAX as u32, i32::MAX as u32).unwrap();
    // Snapshot fields may be mutated by a caller; movement still restores clipping.
    maximum.x = i32::MAX;
    maximum.y = i32::MIN;
    maximum.move_by(i16::MAX, i16::MIN);
    assert_eq!((maximum.x, maximum.y), (i32::MAX - 1, 0));
}

#[test]
fn dispatcher_orders_devices_and_flips_y_into_screen_coordinates() {
    let mut input = Dispatcher::<8>::new(20, 10).unwrap();
    input.feed_keyboard(0x1e);
    send_mouse(&mut input, [0x09, 3, 2]);
    input.feed_keyboard(0x9e);
    assert!(
        matches!(input.pop_event(), Some(Event::Key(event)) if event.state == KeyState::Pressed)
    );
    let Some(Event::Pointer(event)) = input.pop_event() else {
        panic!("expected pointer event");
    };
    assert_eq!((event.x, event.y, event.dx, event.dy), (13, 3, 3, -2));
    assert_eq!(event.buttons.bits(), Buttons::LEFT);
    assert!(
        matches!(input.pop_event(), Some(Event::Key(event)) if event.state == KeyState::Released)
    );
    assert_eq!(input.pop_event(), None);
}

#[test]
fn queue_wraps_without_reordering_and_accepts_exact_capacity() {
    let mut input = Dispatcher::<2>::new(10, 10).unwrap();
    input.feed_keyboard(0x1e);
    input.feed_keyboard(0x30);
    for expected in [0x1e, 0x30] {
        assert!(matches!(input.pop_event(), Some(Event::Key(event)) if event.key.code == expected));
        input.feed_keyboard(0x2e);
    }
    assert!(
        matches!(input.pop_event(), Some(Event::Key(event)) if event.state == KeyState::Pressed)
    );
    assert!(
        matches!(input.pop_event(), Some(Event::Key(event)) if event.state == KeyState::Repeat)
    );
    assert_eq!(input.pop_event(), None);
}

#[test]
fn queue_overflow_invalidates_held_state_and_publishes_reset_first() {
    let mut input = Dispatcher::<2>::new(10, 10).unwrap();
    input.feed_keyboard(0x2a);
    send_mouse(&mut input, [0x09, 1, 0]);
    input.feed_keyboard(0x1e);
    assert_eq!(input.pointer().buttons.bits(), 0);
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::QueueOverflow))
    );
    assert_eq!(input.pop_event(), None);
    input.feed_keyboard(0x1e);
    let Some(Event::Key(event)) = input.pop_event() else {
        panic!("expected fresh key after reset");
    };
    assert_eq!(event.state, KeyState::Pressed);
    assert!(!event.modifiers.shift());
    send_mouse(&mut input, [0x09, 0, 0]);
    assert!(matches!(input.pop_event(), Some(Event::Pointer(event)) if event.changed.bits() == 1));
}

#[test]
fn single_slot_capacity_and_zero_capacity_have_explicit_behavior() {
    assert!(Dispatcher::<0>::new(10, 10).is_none());
    let mut input = Dispatcher::<1>::new(10, 10).unwrap();
    input.feed_keyboard(0x1e);
    input.feed_keyboard(0x30);
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::QueueOverflow))
    );
    input.feed_keyboard(0x30);
    assert!(
        matches!(input.pop_event(), Some(Event::Key(event)) if event.state == KeyState::Pressed)
    );
}

#[test]
fn malformed_input_invalidates_pending_history_and_recovers() {
    let mut input = Dispatcher::<8>::new(10, 10).unwrap();
    input.feed_keyboard(0x2a);
    input.feed_mouse(0);
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::MalformedInput))
    );
    assert_eq!(input.pop_event(), None);
    input.feed_keyboard(0x1e);
    assert!(matches!(input.pop_event(), Some(Event::Key(event)) if !event.modifiers.shift()));
    input.feed_keyboard(0xe1);
    input.feed_keyboard(0x1e);
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::MalformedInput))
    );
}

#[test]
fn disconnect_drops_future_input_and_reconnect_starts_with_clear_state() {
    let mut input = Dispatcher::<8>::new(10, 10).unwrap();
    input.feed_keyboard(0x2a);
    send_mouse(&mut input, [0x09, 1, 0]);
    let position = (input.pointer().x, input.pointer().y);
    input.disconnect();
    assert!(!input.is_connected());
    input.feed_keyboard(0x1e);
    send_mouse(&mut input, [0x09, 10, 10]);
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::ClientDisconnected))
    );
    assert_eq!(input.pop_event(), None);
    assert_eq!((input.pointer().x, input.pointer().y), position);
    assert_eq!(input.pointer().buttons.bits(), 0);
    input.connect();
    assert!(input.is_connected());
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::ClientConnected))
    );
    input.feed_keyboard(0x1e);
    assert!(
        matches!(input.pop_event(), Some(Event::Key(event)) if event.state == KeyState::Pressed && !event.modifiers.shift())
    );
}

#[test]
fn transport_timeout_can_reset_incomplete_prefixes_without_resetting_idle_keys() {
    let mut input = Dispatcher::<8>::new(10, 10).unwrap();
    input.feed_keyboard(0x2a);
    assert!(!input.has_partial());
    input.feed_keyboard(0xe0);
    assert!(input.has_partial());
    assert!(input.keyboard_has_partial_sequence());
    assert!(!input.mouse_has_partial_packet());
    input.reset(ResetReason::DeviceError);
    assert!(!input.has_partial());
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::DeviceError))
    );
    input.feed_mouse(0x08);
    assert!(input.has_partial());
    assert!(input.mouse_has_partial_packet());
    assert!(!input.keyboard_has_partial_sequence());
    input.reset(ResetReason::DeviceError);
    send_mouse(&mut input, [0x08, 1, 0]);
    assert!(!input.has_partial());
    assert_eq!(
        input.pop_event(),
        Some(Event::Reset(ResetReason::DeviceError))
    );
    assert!(matches!(input.pop_event(), Some(Event::Pointer(event)) if event.dx == 1));
}

#[test]
fn cursor_remains_clipped_during_many_movement_packets_and_overflow() {
    let mut input = Dispatcher::<2>::new(7, 5).unwrap();
    for bytes in [
        [0x08, 255, 255],
        [0x38, 0, 0],
        [0x08, 255, 0],
        [0x08, 0, 255],
    ] {
        send_mouse(&mut input, bytes);
        let point = input.pointer();
        assert!((0..7).contains(&point.x));
        assert!((0..5).contains(&point.y));
        input.pop_event();
    }
    let point = input.pointer();
    send_mouse(&mut input, [0xc8, 127, 127]);
    assert_eq!((input.pointer().x, input.pointer().y), (point.x, point.y));
}
