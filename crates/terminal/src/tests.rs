// SPDX-License-Identifier: Apache-2.0
//! Check fragmented ANSI parsing, cursor/erase behavior and bounded history/resize retention.
use super::*;

#[test]
fn replacing_a_producer_recovers_from_unterminated_control_strings() {
    let mut s = Screen::new();
    s.feed(b"old\r\n\x1b[31m\x1b]unterminated");
    s.new_stream();
    s.feed(b"new");
    assert_eq!(s.cell(0, 0).byte, b'o');
    assert_eq!(
        s.cell(1, 0),
        Cell {
            byte: b'n',
            foreground: 7,
            background: 0
        }
    );
}

#[test]
fn fragmented_controls_preserve_cursor_colors_and_erasure() {
    let bytes = b"abc\rZ\x1b[2;4H\x1b[31;44mred\x1b[s\x1b[1;1H!\x1b[u?\x1b[0m\x1b[K";
    for split in 0..=bytes.len() {
        let mut s = Screen::new();
        s.feed(&bytes[..split]);
        s.feed(&bytes[split..]);
        assert_eq!(s.cell(0, 0).byte, b'!');
        assert_eq!(s.cell(0, 1).byte, b'b');
        assert_eq!(
            s.cell(1, 3),
            Cell {
                byte: b'r',
                foreground: 1,
                background: 4
            }
        );
        assert_eq!(s.cell(1, 6).byte, b'?');
        assert_eq!(s.cell(1, 7), Cell::EMPTY);
    }
}

#[test]
fn delayed_wrap_carriage_return_and_backspace_do_not_scroll_prematurely() {
    let mut s = Screen::new();
    s.resize(2, 3);
    s.feed(b"abc\rX\n\r123");
    assert_eq!(s.history_len(), 0);
    assert_eq!(s.cell(0, 0).byte, b'X');
    s.feed(b"4");
    assert_eq!(s.history_len(), 1);
    assert_eq!(s.cell(0, 0).byte, b'1');
    assert_eq!(s.cell(1, 0).byte, b'4');
    s.feed(b"\x08Z");
    assert_eq!(s.cell(1, 0).byte, b'Z');
}

#[test]
fn sgr_does_not_cancel_delayed_wrap_and_unknown_extended_colors_are_consumed() {
    let mut s = Screen::new();
    s.resize(2, 3);
    s.feed(b"abc\x1b[31mZ");
    assert_eq!(
        s.cell(1, 0),
        Cell {
            byte: b'Z',
            foreground: 1,
            background: 0
        }
    );
    s.feed(b"\x1b[38;2;0;0;0mX");
    assert_eq!(s.cell(1, 1).foreground, 1);
}

#[test]
fn history_wraps_without_growth_and_view_remains_bounded_during_output() {
    let mut s = Screen::new();
    s.resize(2, 20);
    for _ in 0..300 {
        s.feed(b"line\r\n");
    }
    assert_eq!(s.history_len(), HISTORY);
    assert_eq!(s.dropped(), 43);
    s.scroll_view(i32::MAX);
    assert_eq!(s.view_offset(), HISTORY);
    s.feed(b"new\r\n");
    assert_eq!(s.view_offset(), HISTORY);
    s.scroll_view(i32::MIN);
    assert_eq!(s.view_offset(), 0);
    assert!(size_of::<Screen>() < 45_000);
}

#[test]
fn resize_clips_pads_and_clamps_saved_cursor_without_reflow() {
    let mut s = Screen::new();
    s.feed(b"abcd\x1b[26;52H\x1b[s");
    s.resize(1, 2);
    s.feed(b"\x1b[u");
    assert_eq!(s.cursor(), Some((0, 1)));
    s.resize(26, 52);
    assert_eq!(s.cell(0, 0).byte, b'a');
    assert_eq!(s.cell(0, 2).byte, b' ');
    s.feed(b"\x1b[?25l");
    assert_eq!(s.cursor(), None);
    s.feed(b"\x1b[?25h");
    assert!(s.cursor().is_some());
}

#[test]
fn oversized_csi_unsupported_strings_and_utf8_never_leak_control_payload() {
    let mut s = Screen::new();
    s.feed(b"\x1b[9999999999999999999999999999999999H");
    s.feed(b"\x1b[1;2;3;4;5;6;7;8;9m");
    for _ in 0..10_000 {
        s.feed(b"\x1b]ignored title\x1b\\");
    }
    s.feed(b"A\xe4\xb8");
    s.feed(b"\xadB\x1bPignored\x1b\\C");
    assert_eq!(s.cursor(), Some((0, 4)));
    assert_eq!(s.cell(0, 1).byte, b'?');
    assert_eq!(s.cell(0, 3).byte, b'C');
}
