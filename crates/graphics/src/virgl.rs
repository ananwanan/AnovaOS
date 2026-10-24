// SPDX-License-Identifier: Apache-2.0
//! Encode the bounded VirGL v1 offscreen clear stream without executing floating-point code.
//! Device negotiation, DMA lifetime, fencing and readback remain kernel-driver responsibilities.

/// The experimental renderer accepts at most this many pixels along either axis.
pub const MAX_DIMENSION: u32 = 64;

/// Convert an eight-bit UNORM channel into IEEE-754 bits using integer arithmetic only.
/// The division rounds to the nearest representable value; an odd denominator excludes ties.
pub fn normalized_channel(channel: u8) -> u32 {
    if channel == 0 {
        return 0;
    }
    if channel == 255 {
        return 0x3f80_0000;
    }
    let highest = 7 - channel.leading_zeros();
    let significand = ((u64::from(channel) << (31 - highest)) + 127) / 255;
    ((highest + 119) << 23) | (significand as u32 - (1 << 23))
}

/// Create surface 1 over resource 1, bind it as color attachment 0, then clear it.
/// Values follow the public VirGL v1 wire protocol; high RGB bits are ignored.
pub fn clear_stream(rgb: u32) -> [u32; 19] {
    [
        1 | (8 << 8) | (5 << 16), // CREATE_OBJECT, SURFACE, five payload words.
        1,                        // Surface handle.
        1,                        // Resource handle.
        1,                        // B8G8R8A8_UNORM.
        0,                        // Mipmap level.
        0,                        // First/last layer.
        5 | (3 << 16),            // SET_FRAMEBUFFER_STATE, three payload words.
        1,                        // One color attachment.
        0,                        // No depth/stencil attachment.
        1,                        // Surface handle.
        7 | (8 << 16),            // CLEAR, eight payload words.
        4,                        // PIPE_CLEAR_COLOR0.
        normalized_channel((rgb >> 16) as u8),
        normalized_channel((rgb >> 8) as u8),
        normalized_channel(rgb as u8),
        0x3f80_0000, // Opaque alpha.
        0,           // Depth, low double word.
        0,           // Depth, high double word.
        0,           // Stencil.
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_channel_encoding_matches_independent_float_reference() {
        for channel in 0..=255_u8 {
            assert_eq!(
                normalized_channel(channel),
                (f32::from(channel) / 255.0).to_bits(),
                "Incorrect channel {channel}"
            );
        }
    }

    #[test]
    fn clear_stream_has_exact_wire_lengths_and_opaque_rgb_channels() {
        let stream = clear_stream(0xab12_80ff);
        let mut cursor = 0;
        for (command, object, words) in [(1, 8, 5), (5, 0, 3), (7, 0, 8)] {
            assert_eq!(stream[cursor] & 255, command);
            assert_eq!((stream[cursor] >> 8) & 255, object);
            assert_eq!(stream[cursor] >> 16, words);
            cursor += words as usize + 1;
        }
        assert_eq!(cursor, stream.len());
        assert_eq!(stream[11], 4);
        assert_eq!(stream[12], (18.0_f32 / 255.0).to_bits());
        assert_eq!(stream[13], (128.0_f32 / 255.0).to_bits());
        assert_eq!(stream[14], 1.0_f32.to_bits());
        assert_eq!(stream[15], 1.0_f32.to_bits());
        assert_eq!(stream[16..], [0; 3]);
    }
}
