// SPDX-License-Identifier: Apache-2.0
//! Verify software clipping, untrusted layout/source rejection and exact RGB packing on the host.

extern crate std;

use super::*;
use std::{vec, vec::Vec};

fn rgb_layout(width: u64, height: u64, pitch: u64) -> Result<Layout, Error> {
    Layout::new(width, height, pitch, 32, 8, 16, 8, 8, 8, 0)
}

#[test]
fn layout_accepts_bounded_rgb_and_padded_rows() {
    let layout = rgb_layout(3, 2, 16).unwrap();
    assert_eq!(layout.width(), 3);
    assert_eq!(layout.height(), 2);
    assert_eq!(layout.pitch(), 16);
    assert_eq!(layout.pixel_count(), 6);
    assert_eq!(layout.byte_len(), 32);
    assert_eq!(layout.bytes_per_pixel(), 4);
    assert_eq!(layout.pixel_offset(0, 0), Some(0));
    assert_eq!(layout.pixel_offset(2, 1), Some(24));
    assert_eq!(layout.pixel_offset(3, 0), None);
    assert_eq!(layout.pixel_offset(0, 2), None);
    assert_eq!(layout.pack_rgb(0xee123456), 0x00123456);
}

#[test]
fn layout_rejects_empty_huge_and_resource_exhausting_dimensions() {
    assert_eq!(rgb_layout(0, 1, 4), Err(Error::ZeroDimension));
    assert_eq!(rgb_layout(1, 0, 4), Err(Error::ZeroDimension));
    assert_eq!(rgb_layout(u64::MAX, 2, 16), Err(Error::DimensionLimit));
    assert_eq!(rgb_layout(1, u64::MAX, 16), Err(Error::DimensionLimit));
    assert_eq!(rgb_layout(4096, 4096, 16384), Err(Error::PixelLimit));
    assert_eq!(
        rgb_layout(1920, 1080, 7680).unwrap().pixel_count(),
        2_073_600
    );
}

#[test]
fn layout_rejects_short_pitches_huge_buffers_and_length_overflow() {
    assert_eq!(rgb_layout(2, 2, 7), Err(Error::InvalidPitch));
    assert_eq!(rgb_layout(1, 2, u64::MAX), Err(Error::ArithmeticOverflow));
    assert_eq!(rgb_layout(1, 1, u64::MAX), Err(Error::FramebufferLimit));
    assert_eq!(
        rgb_layout(1, 1, MAX_FRAMEBUFFER_BYTES as u64 + 1),
        Err(Error::FramebufferLimit)
    );
    assert!(rgb_layout(1, 1, MAX_FRAMEBUFFER_BYTES as u64).is_ok());
}

#[test]
fn layout_rejects_unsupported_channel_widths_shifts_overlap_and_bpp() {
    for bpp in [0, 8, 15, 16, 30, 64, u16::MAX] {
        assert_eq!(
            Layout::new(1, 1, 8, bpp, 8, 16, 8, 8, 8, 0),
            Err(Error::UnsupportedFormat)
        );
    }
    for (red_size, green_size, blue_size) in [(0, 8, 8), (8, 7, 8), (8, 8, 9)] {
        assert_eq!(
            Layout::new(1, 1, 4, 32, red_size, 16, green_size, 8, blue_size, 0),
            Err(Error::UnsupportedFormat)
        );
    }
    for (red_shift, green_shift, blue_shift) in [(16, 8, 8), (8, 4, 0), (24, 8, 255)] {
        assert_eq!(
            Layout::new(1, 1, 4, 32, 8, red_shift, 8, green_shift, 8, blue_shift),
            Err(Error::UnsupportedFormat)
        );
    }
    assert_eq!(
        Layout::new(1, 1, 3, 24, 8, 24, 8, 8, 8, 0),
        Err(Error::UnsupportedFormat)
    );
}

#[test]
fn layout_accepts_distinct_unaligned_eight_bit_channels_in_32_bits() {
    let layout = Layout::new(1, 1, 4, 32, 8, 17, 8, 9, 8, 1).unwrap();
    assert_eq!(layout.pack_rgb(0x123456), 0x002468ac);
    let high_red = Layout::new(1, 1, 4, 32, 8, 24, 8, 16, 8, 8).unwrap();
    assert_eq!(high_red.pack_rgb(0x123456), 0x12345600);
}

#[test]
fn surface_validation_leaves_rejected_storage_unchanged() {
    let mut storage = [0xaabbccdd; 12];
    assert!(matches!(
        Surface::new(0, 3, &mut storage),
        Err(Error::ZeroDimension)
    ));
    assert!(matches!(
        Surface::new(4097, 1, &mut storage),
        Err(Error::DimensionLimit)
    ));
    assert!(matches!(
        Surface::new(4096, 4096, &mut storage),
        Err(Error::PixelLimit)
    ));
    assert!(matches!(
        Surface::new(4, 4, &mut storage),
        Err(Error::BufferTooSmall)
    ));
    assert_eq!(storage, [0xaabbccdd; 12]);
}

#[test]
fn pixel_and_rectangle_clipping_preserve_buffer_canaries() {
    let mut storage = [0xdeadbeef; 18];
    {
        let mut surface = Surface::new(4, 4, &mut storage[1..]).unwrap();
        assert_eq!(surface.pixels(), &[0; 16]);
        surface.fill_rect(-2, -1, 4, 3, 0xff123456);
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(
                    surface.get_pixel(x, y),
                    Some(if x < 2 && y < 2 { 0x123456 } else { 0 })
                );
            }
        }
        assert!(!surface.pixel(-1, 0, 1));
        assert!(!surface.pixel(0, -1, 1));
        assert!(!surface.pixel(4, 0, 1));
        assert!(!surface.pixel(i64::MAX, i64::MIN, 1));
        assert!(surface.pixel(3, 3, 0xffabcdef));
        assert_eq!(surface.get_pixel(3, 3), Some(0xabcdef));
        assert_eq!(surface.get_pixel(4, 3), None);
        assert_eq!(surface.get_pixel(3, 4), None);
    }
    assert_eq!(storage[0], 0xdeadbeef);
    assert_eq!(storage[17], 0xdeadbeef);
}

#[test]
fn rectangles_match_independent_wide_coordinate_reference() {
    let origins = [i64::MIN, i64::MIN + 5, -20, -4, -1, 0, 1, 5, 9, i64::MAX];
    let sizes = [0, 1, 4, 20, u32::MAX];
    let mut storage = [0; 35];
    let mut surface = Surface::new(7, 5, &mut storage).unwrap();
    for x in origins {
        for y in origins {
            for width in sizes {
                for height in sizes {
                    surface.fill_rect(0, 0, 7, 5, 0);
                    surface.fill_rect(x, y, width, height, 0x123456);
                    for py in 0..5 {
                        for px in 0..7 {
                            let inside = i128::from(px) >= i128::from(x)
                                && i128::from(px) < i128::from(x) + i128::from(width)
                                && i128::from(py) >= i128::from(y)
                                && i128::from(py) < i128::from(y) + i128::from(height);
                            assert_eq!(
                                surface.get_pixel(px, py),
                                Some(if inside { 0x123456 } else { 0 }),
                                "rectangle ({x}, {y}, {width}, {height}) pixel ({px}, {py})"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn padded_blit_clips_source_coordinates_and_ignores_padding() {
    let source = [1, 2, 3, 0xdeadbeef, 4, 5, 6, 0xdeadbeef, 7, 8, 9];
    let mut storage = [0; 9];
    let mut surface = Surface::new(3, 3, &mut storage).unwrap();
    surface.blit(-1, -1, &source, 3, 3, 4).unwrap();
    assert_eq!(surface.pixels(), &[5, 6, 0, 8, 9, 0, 0, 0, 0]);
    surface.fill_rect(0, 0, 3, 3, 0);
    surface.blit(2, 1, &source, 3, 3, 4).unwrap();
    assert_eq!(surface.pixels(), &[0, 0, 0, 0, 0, 1, 0, 0, 4]);
}

#[test]
fn blits_match_independent_source_coordinate_reference() {
    let source = [0xff000001, 2, 3, 999, 4, 5, 6, 999, 7, 8, 9];
    let origins = [i64::MIN, -4, -2, -1, 0, 1, 3, 5, i64::MAX];
    let mut storage = [0; 20];
    let mut surface = Surface::new(5, 4, &mut storage).unwrap();
    for x in origins {
        for y in origins {
            surface.fill_rect(0, 0, 5, 4, 0);
            surface.blit(x, y, &source, 3, 3, 4).unwrap();
            for py in 0..4 {
                for px in 0..5 {
                    let sx = i128::from(px) - i128::from(x);
                    let sy = i128::from(py) - i128::from(y);
                    let expected = if (0..3).contains(&sx) && (0..3).contains(&sy) {
                        source[sy as usize * 4 + sx as usize] & RGB_MASK
                    } else {
                        0
                    };
                    assert_eq!(surface.get_pixel(px, py), Some(expected));
                }
            }
        }
    }
}

#[test]
fn blit_rejects_bad_stride_short_source_and_overflow_before_mutation() {
    let mut storage = [0; 9];
    let mut surface = Surface::new(3, 3, &mut storage).unwrap();
    surface.fill_rect(0, 0, 3, 3, 0x112233);
    for (x, y) in [(0, 0), (-100, -100), (i64::MAX, i64::MIN)] {
        assert_eq!(
            surface.blit(x, y, &[1; 9], 3, 3, 2),
            Err(Error::InvalidSourceStride)
        );
        assert_eq!(
            surface.blit(x, y, &[1; 8], 3, 3, 3),
            Err(Error::BufferTooSmall)
        );
        assert_eq!(
            surface.blit(x, y, &[1; 9], 3, 3, usize::MAX),
            Err(Error::ArithmeticOverflow)
        );
        assert_eq!(surface.pixels(), &[0x112233; 9]);
    }
    surface.blit(0, 0, &[], 0, u32::MAX, usize::MAX).unwrap();
    surface.blit(0, 0, &[], u32::MAX, 0, 0).unwrap();
    assert_eq!(surface.pixels(), &[0x112233; 9]);
}

#[test]
fn rgb_bgr_and_24_bit_encoding_leave_padding_and_guards_unchanged() {
    let mut source = [0; 4];
    let mut surface = Surface::new(2, 2, &mut source).unwrap();
    surface
        .blit(0, 0, &[0x123456, 0xabcdef, 0x102030, 0xff0080], 2, 2, 2)
        .unwrap();
    for (bpp, red_shift, blue_shift, expected) in [
        (32, 16, 0, vec![0x56, 0x34, 0x12, 0, 0xef, 0xcd, 0xab, 0]),
        (32, 0, 16, vec![0x12, 0x34, 0x56, 0, 0xab, 0xcd, 0xef, 0]),
        (24, 16, 0, vec![0x56, 0x34, 0x12, 0xef, 0xcd, 0xab]),
        (24, 0, 16, vec![0x12, 0x34, 0x56, 0xab, 0xcd, 0xef]),
    ] {
        let row_bytes = expected.len();
        let pitch = row_bytes + 3;
        let layout =
            Layout::new(2, 2, pitch as u64, bpp, 8, red_shift, 8, 8, 8, blue_shift).unwrap();
        let mut storage = vec![0xa5; layout.byte_len() + 2];
        surface.encode(&layout, &mut storage[1..]).unwrap();
        assert_eq!(&storage[1..1 + row_bytes], expected);
        assert_eq!(&storage[1 + row_bytes..1 + pitch], &[0xa5; 3]);
        assert_eq!(&storage[1 + pitch + row_bytes..1 + 2 * pitch], &[0xa5; 3]);
        for y in 0..2 {
            for x in 0..2 {
                let offset = 1 + layout.pixel_offset(x, y).unwrap();
                let color = layout
                    .pack_rgb(surface.get_pixel(x, y).unwrap())
                    .to_le_bytes();
                assert_eq!(
                    &storage[offset..offset + layout.bytes_per_pixel()],
                    &color[..bpp as usize / 8]
                );
            }
        }
        assert_eq!(storage[0], 0xa5);
        assert_eq!(storage[storage.len() - 1], 0xa5);
    }
}

#[test]
fn present_rejects_mismatched_layout_without_calling_writer() {
    let mut storage = [0; 6];
    let surface = Surface::new(3, 2, &mut storage).unwrap();
    let mut writes = 0;
    assert_eq!(
        surface.present(&rgb_layout(2, 3, 8).unwrap(), |_, _| writes += 1),
        Err(Error::DimensionMismatch)
    );
    assert_eq!(writes, 0);
    let mut output = [0xab; 23];
    assert_eq!(
        surface.encode(&rgb_layout(3, 2, 12).unwrap(), &mut output),
        Err(Error::BufferTooSmall)
    );
    assert_eq!(output, [0xab; 23]);
}

#[test]
fn checksum_is_canonical_rgb_fnv1a_and_ignores_input_high_bits() {
    let mut storage = [0];
    let mut surface = Surface::new(1, 1, &mut storage).unwrap();
    surface.pixel(0, 0, 0xff123456);
    assert_eq!(surface.checksum(), 0x7486b218c3c86edf);
}

#[test]
fn pattern_has_independently_known_pixels_and_stable_checksum() {
    let mut storage = vec![0; 96 * 72];
    let mut surface = Surface::new(96, 72, &mut storage).unwrap();
    surface.draw_test_pattern();
    assert_eq!(surface.get_pixel(0, 0), Some(0xffffff));
    assert_eq!(surface.get_pixel(95, 0), Some(0));
    for (x, color) in [
        (6, 0xffffff),
        (18, 0xffff00),
        (30, 0x00ffff),
        (42, 0x00ff00),
        (54, 0xff00ff),
        (66, 0xff0000),
        (78, 0x0000ff),
        (90, 0),
    ] {
        assert_eq!(surface.get_pixel(x, 10), Some(color));
    }
    assert_eq!(surface.get_pixel(1, 25), Some(0x101820));
    assert_eq!(surface.get_pixel(12, 30), Some(0xf0f4f8));
    assert_eq!(surface.get_pixel(16, 34), Some(0x24405c));
    assert_eq!(surface.get_pixel(0, 54), Some(0xe63946));
    assert_eq!(surface.get_pixel(20, 60), Some(0x101820));
    assert_eq!(surface.get_pixel(45, 60), Some(0xff00ff));
    assert_eq!(surface.get_pixel(46, 60), Some(0x00ffff));
    assert_eq!(surface.get_pixel(95, 36), Some(0xff00ff));
    assert_eq!(surface.get_pixel(0, 68), Some(0xff00ff));
    assert_eq!(surface.get_pixel(95, 71), Some(0x2a9d8f));
    assert_eq!(surface.checksum(), 0x9b9f73fb82553229);
}

#[test]
fn pattern_and_encode_preserve_guards_on_tiny_and_odd_surfaces() {
    for width in [1, 2, 7, 17] {
        for height in [1, 2, 5, 19] {
            let count = width * height;
            let mut storage = vec![0xdeadc0de; count as usize + 2];
            {
                let mut surface = Surface::new(width, height, &mut storage[1..]).unwrap();
                surface.draw_test_pattern();
                let layout = rgb_layout(
                    u64::from(width),
                    u64::from(height),
                    u64::from(width) * 4 + 1,
                )
                .unwrap();
                let mut encoded = vec![0xfe; layout.byte_len() + 2];
                surface.encode(&layout, &mut encoded[1..]).unwrap();
                assert_eq!(encoded[0], 0xfe);
                assert_eq!(encoded[encoded.len() - 1], 0xfe);
                for row in 0..height as usize {
                    assert_eq!(encoded[1 + row * layout.pitch() + width as usize * 4], 0xfe);
                }
            }
            assert_eq!(storage[0], 0xdeadc0de);
            assert_eq!(storage[storage.len() - 1], 0xdeadc0de);
        }
    }
}

#[test]
fn all_channel_permutations_round_trip_without_touching_padding() {
    let colors = [0, 0xffffff, 0x123456, 0xff010080, 0x00ff00];
    let mut storage = [0; 5];
    let mut surface = Surface::new(5, 1, &mut storage).unwrap();
    surface.blit(0, 0, &colors, 5, 1, 5).unwrap();
    for bpp in [24, 32] {
        for red_shift in [0, 8, 16] {
            for green_shift in [0, 8, 16] {
                for blue_shift in [0, 8, 16] {
                    if red_shift == green_shift
                        || red_shift == blue_shift
                        || green_shift == blue_shift
                    {
                        continue;
                    }
                    let layout =
                        Layout::new(5, 1, 24, bpp, 8, red_shift, 8, green_shift, 8, blue_shift)
                            .unwrap();
                    let mut output = [0xab; 24];
                    surface.encode(&layout, &mut output).unwrap();
                    for (index, original) in colors.into_iter().enumerate() {
                        let start = index * layout.bytes_per_pixel();
                        let mut bytes = [0; 4];
                        bytes[..layout.bytes_per_pixel()]
                            .copy_from_slice(&output[start..start + layout.bytes_per_pixel()]);
                        let packed = u32::from_le_bytes(bytes);
                        let recovered = ((packed >> red_shift) & 0xff) << 16
                            | ((packed >> green_shift) & 0xff) << 8
                            | ((packed >> blue_shift) & 0xff);
                        assert_eq!(recovered, original & RGB_MASK);
                    }
                    assert!(
                        output[5 * layout.bytes_per_pixel()..]
                            .iter()
                            .all(|byte| *byte == 0xab)
                    );
                }
            }
        }
    }
}

#[test]
fn no_std_renderer_has_no_implicit_allocation_requirement() {
    // A fixed stack buffer exercises the complete render/present flow without Vec or alloc.
    let mut pixels = [0; 16 * 12];
    let mut surface = Surface::new(16, 12, &mut pixels).unwrap();
    surface.draw_test_pattern();
    let mut bytes = [0; 16 * 12 * 3];
    let layout = Layout::new(16, 12, 48, 24, 8, 16, 8, 8, 8, 0).unwrap();
    surface.encode(&layout, &mut bytes).unwrap();
    assert_eq!(&bytes[..3], &[255, 255, 255]);
}

#[test]
fn presentation_visits_only_visible_offsets_once_and_in_row_order() {
    let mut pixels = [0; 6];
    let surface = Surface::new(3, 2, &mut pixels).unwrap();
    let layout = Layout::new(3, 2, 13, 24, 8, 16, 8, 8, 8, 0).unwrap();
    let mut offsets = Vec::new();
    surface
        .present(&layout, |offset, byte| {
            assert_eq!(byte, 0);
            offsets.push(offset);
        })
        .unwrap();
    assert_eq!(
        offsets,
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 13, 14, 15, 16, 17, 18, 19, 20, 21
        ]
    );
}
