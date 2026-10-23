// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Validate framebuffer layouts and draw into caller-owned RGB buffers without device access.
//!
//! Colors use `0x00RRGGBB`; unused high bits are ignored. Layout validation and byte packing
//! are separate from mapping, cache policy, volatile device writes and display synchronization,
//! which remain the display driver's responsibility. Rendering does not allocate or use SIMD.

/// The bounded software path accepts at most this many pixels on either axis.
pub const MAX_DIMENSION: u32 = 4096;
/// An RGB back buffer occupies at most eight MiB of the kernel's fixed heap.
pub const MAX_PIXELS: usize = 2 * 1024 * 1024;
/// Include row padding in the mapped framebuffer's sixteen MiB limit.
pub const MAX_FRAMEBUFFER_BYTES: usize = 16 * 1024 * 1024;

const RGB_MASK: u32 = 0x00ff_ffff;

/// Rejections are independent of allocation, physical mapping and device lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    ZeroDimension,
    DimensionLimit,
    PixelLimit,
    FramebufferLimit,
    ArithmeticOverflow,
    UnsupportedFormat,
    InvalidPitch,
    BufferTooSmall,
    DimensionMismatch,
    InvalidSourceStride,
}

/// A checked little-endian RGB framebuffer description, including padded row length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layout {
    width: u32,
    height: u32,
    pitch: usize,
    byte_len: usize,
    pixel_count: usize,
    bytes_per_pixel: usize,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
}

impl Layout {
    /// Accept only packed 24/32-bit RGB with three distinct eight-bit channels.
    ///
    /// Width, height and pitch are kept wide until their arithmetic and resource bounds
    /// have been checked. Callers must separately validate the boot protocol memory model,
    /// address, mapping permissions and the complete `byte_len()` address range.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: u64,
        height: u64,
        pitch: u64,
        bits_per_pixel: u16,
        red_size: u8,
        red_shift: u8,
        green_size: u8,
        green_shift: u8,
        blue_size: u8,
        blue_shift: u8,
    ) -> Result<Self, Error> {
        let (width, height, pixel_count) = checked_dimensions(width, height)?;
        if !matches!(bits_per_pixel, 24 | 32)
            || red_size != 8
            || green_size != 8
            || blue_size != 8
            || u16::from(red_shift) + 8 > bits_per_pixel
            || u16::from(green_shift) + 8 > bits_per_pixel
            || u16::from(blue_shift) + 8 > bits_per_pixel
        {
            return Err(Error::UnsupportedFormat);
        }
        let red_mask = 0xff_u32 << red_shift;
        let green_mask = 0xff_u32 << green_shift;
        let blue_mask = 0xff_u32 << blue_shift;
        if red_mask & green_mask != 0 || red_mask & blue_mask != 0 || green_mask & blue_mask != 0 {
            return Err(Error::UnsupportedFormat);
        }
        let bytes_per_pixel = usize::from(bits_per_pixel / 8);
        let row_bytes = u64::from(width)
            .checked_mul(bytes_per_pixel as u64)
            .ok_or(Error::ArithmeticOverflow)?;
        if pitch < row_bytes {
            return Err(Error::InvalidPitch);
        }
        let byte_len = pitch
            .checked_mul(u64::from(height))
            .ok_or(Error::ArithmeticOverflow)?;
        let pitch = usize::try_from(pitch).map_err(|_| Error::ArithmeticOverflow)?;
        let byte_len = usize::try_from(byte_len).map_err(|_| Error::ArithmeticOverflow)?;
        if byte_len > MAX_FRAMEBUFFER_BYTES {
            return Err(Error::FramebufferLimit);
        }
        Ok(Self {
            width,
            height,
            pitch,
            byte_len,
            pixel_count,
            bytes_per_pixel,
            red_shift,
            green_shift,
            blue_shift,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn pitch(self) -> usize {
        self.pitch
    }

    /// The complete mapped range includes padding after the final visible row.
    pub const fn byte_len(self) -> usize {
        self.byte_len
    }

    pub const fn pixel_count(self) -> usize {
        self.pixel_count
    }

    pub const fn bytes_per_pixel(self) -> usize {
        self.bytes_per_pixel
    }

    /// Encode canonical RGB into the validated device channels; unused bits are zero.
    pub const fn pack_rgb(self, color: u32) -> u32 {
        ((color >> 16) & 0xff) << self.red_shift
            | ((color >> 8) & 0xff) << self.green_shift
            | (color & 0xff) << self.blue_shift
    }

    /// Return a pixel's checked first-byte offset, excluding row padding.
    pub fn pixel_offset(self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        // Layout construction bounds every row and column below byte_len.
        Some(y as usize * self.pitch + x as usize * self.bytes_per_pixel)
    }
}

fn checked_dimensions(width: u64, height: u64) -> Result<(u32, u32, usize), Error> {
    if width == 0 || height == 0 {
        return Err(Error::ZeroDimension);
    }
    if width > u64::from(MAX_DIMENSION) || height > u64::from(MAX_DIMENSION) {
        return Err(Error::DimensionLimit);
    }
    let pixels = width.checked_mul(height).ok_or(Error::ArithmeticOverflow)?;
    let pixels = usize::try_from(pixels).map_err(|_| Error::ArithmeticOverflow)?;
    if pixels > MAX_PIXELS {
        return Err(Error::PixelLimit);
    }
    Ok((width as u32, height as u32, pixels))
}

/// A tightly packed, caller-owned software back buffer, with no hardware pointer or allocator.
pub struct Surface<'a> {
    width: u32,
    height: u32,
    pixels: &'a mut [u32],
}

impl<'a> Surface<'a> {
    /// Clear and borrow the visible pixel prefix; leave any remaining backing storage untouched.
    pub fn new(width: u32, height: u32, pixels: &'a mut [u32]) -> Result<Self, Error> {
        let (_, _, count) = checked_dimensions(u64::from(width), u64::from(height))?;
        let pixels = pixels.get_mut(..count).ok_or(Error::BufferTooSmall)?;
        pixels.fill(0);
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        self.pixels
    }

    /// Return a canonical RGB pixel or `None` outside the visible surface.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(self.pixels[y as usize * self.width as usize + x as usize])
    }

    /// Clip signed coordinates before indexing. Return whether a pixel was written.
    pub fn pixel(&mut self, x: i64, y: i64, color: u32) -> bool {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return false;
        }
        self.pixels[y as usize * self.width as usize + x as usize] = color & RGB_MASK;
        true
    }

    /// Fill the visible intersection; empty and completely clipped rectangles do nothing.
    pub fn fill_rect(&mut self, x: i64, y: i64, width: u32, height: u32, color: u32) {
        let Some(rect) = clipped_rect(x, y, width, height, self.width, self.height) else {
            return;
        };
        let stride = self.width as usize;
        for row in rect.top..rect.bottom {
            self.pixels[row * stride + rect.left..row * stride + rect.right].fill(color & RGB_MASK);
        }
    }

    /// Copy a source RGB image with stride in pixels, clipping both axes at the destination.
    ///
    /// Validate the complete source image before changing any destination pixel, including
    /// when the destination is completely clipped. The final source row needs only its
    /// visible width; source padding is never read. Zero-sized images are no-ops.
    pub fn blit(
        &mut self,
        x: i64,
        y: i64,
        source: &[u32],
        source_width: u32,
        source_height: u32,
        source_stride: usize,
    ) -> Result<(), Error> {
        if source_width == 0 || source_height == 0 {
            return Ok(());
        }
        if source_stride < source_width as usize {
            return Err(Error::InvalidSourceStride);
        }
        let source_len = (source_height as usize - 1)
            .checked_mul(source_stride)
            .and_then(|offset| offset.checked_add(source_width as usize))
            .ok_or(Error::ArithmeticOverflow)?;
        if source.len() < source_len {
            return Err(Error::BufferTooSmall);
        }
        let Some(rect) = clipped_rect(x, y, source_width, source_height, self.width, self.height)
        else {
            return Ok(());
        };
        // A visible intersection guarantees these differences are nonnegative and no
        // larger than the checked source dimensions, including at extreme signed origins.
        let source_x = (rect.left as i128 - i128::from(x)) as usize;
        let source_y = (rect.top as i128 - i128::from(y)) as usize;
        let count = rect.right - rect.left;
        for row in 0..rect.bottom - rect.top {
            let from = (source_y + row) * source_stride + source_x;
            let to = (rect.top + row) * self.width as usize + rect.left;
            for column in 0..count {
                self.pixels[to + column] = source[from + column] & RGB_MASK;
            }
        }
        Ok(())
    }

    /// Present visible bytes through a caller-supplied writer, leaving row padding untouched.
    ///
    /// The writer receives offsets strictly below `layout.byte_len()`. A device driver may
    /// implement it with byte-sized volatile writes after validating the mapped address range.
    /// This synchronous CPU copy provides no vertical-blank or atomic-scanout guarantee.
    pub fn present(&self, layout: &Layout, mut write: impl FnMut(usize, u8)) -> Result<(), Error> {
        if self.width != layout.width || self.height != layout.height {
            return Err(Error::DimensionMismatch);
        }
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let color = self.pixels[y * self.width as usize + x];
                let bytes = layout.pack_rgb(color).to_le_bytes();
                let offset = y * layout.pitch + x * layout.bytes_per_pixel;
                for (byte, value) in bytes.iter().take(layout.bytes_per_pixel).enumerate() {
                    write(offset + byte, *value);
                }
            }
        }
        Ok(())
    }

    /// Encode into a normal-memory framebuffer for host tests or non-device consumers.
    pub fn encode(&self, layout: &Layout, output: &mut [u8]) -> Result<(), Error> {
        if output.len() < layout.byte_len {
            return Err(Error::BufferTooSmall);
        }
        self.present(layout, |offset, byte| output[offset] = byte)
    }

    /// FNV-1a over canonical R, G, B bytes in row order, independent of device packing/padding.
    pub fn checksum(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for pixel in self.pixels.iter() {
            for shift in [16, 8, 0] {
                hash ^= u64::from((pixel >> shift) & 0xff);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    /// Render reproducible bars, nested rectangles, clipped edges and a padded-source blit.
    ///
    /// This boot diagnostic has no text/font/window policy. Both host reference generation
    /// and the display driver use this routine; tests separately pin its pixels and checksum.
    pub fn draw_test_pattern(&mut self) {
        self.fill_rect(0, 0, self.width, self.height, 0x101820);
        let bars = [
            0xffffff, 0xffff00, 0x00ffff, 0x00ff00, 0xff00ff, 0xff0000, 0x0000ff, 0,
        ];
        for (index, color) in bars.into_iter().enumerate() {
            let start = self.width * index as u32 / 8;
            let end = self.width * (index as u32 + 1) / 8;
            self.fill_rect(i64::from(start), 0, end - start, self.height / 3, color);
        }
        let left = self.width / 8;
        let top = self.height * 5 / 12;
        let width = self.width * 3 / 4;
        let height = self.height / 3;
        self.fill_rect(i64::from(left), i64::from(top), width, height, 0xf0f4f8);
        self.fill_rect(
            i64::from(left + 4),
            i64::from(top + 4),
            width.saturating_sub(8),
            height.saturating_sub(8),
            0x24405c,
        );
        self.fill_rect(-12, i64::from(self.height * 3 / 4), 32, 24, 0xe63946);
        self.fill_rect(
            i64::from(self.width) - 20,
            i64::from(self.height) - 12,
            40,
            24,
            0x2a9d8f,
        );
        let tile = [
            0xff00ff, 0x00ffff, 0xff00ff, 0x00ffff, 0xff00ff, 0x00ffff, 0x123456, 0x123456,
            0x00ffff, 0xff00ff, 0x00ffff, 0xff00ff, 0x00ffff, 0xff00ff, 0x123456, 0x123456,
            0xff00ff, 0x00ffff, 0xff00ff, 0x00ffff, 0xff00ff, 0x00ffff, 0x123456, 0x123456,
            0x00ffff, 0xff00ff, 0x00ffff, 0xff00ff, 0x00ffff, 0xff00ff,
        ];
        // The fixed source has 6 visible pixels per row and a checked stride of 8.
        // All calls are valid independent of clipping and deliberately avoid panicking.
        let _ = self.blit(
            i64::from(self.width / 2) - 3,
            i64::from(self.height * 5 / 6),
            &tile,
            6,
            4,
            8,
        );
        let _ = self.blit(-2, i64::from(self.height) - 4, &tile, 6, 4, 8);
        let _ = self.blit(
            i64::from(self.width) - 3,
            i64::from(self.height / 2),
            &tile,
            6,
            4,
            8,
        );
        self.pixel(0, 0, 0xffffff);
        self.pixel(i64::from(self.width) - 1, 0, 0x000000);
    }
}

struct ClippedRect {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

fn clipped_rect(
    x: i64,
    y: i64,
    width: u32,
    height: u32,
    surface_width: u32,
    surface_height: u32,
) -> Option<ClippedRect> {
    let left = x.max(0);
    let top = y.max(0);
    let right = x
        .saturating_add(i64::from(width))
        .min(i64::from(surface_width));
    let bottom = y
        .saturating_add(i64::from(height))
        .min(i64::from(surface_height));
    if left >= right || top >= bottom {
        return None;
    }
    Some(ClippedRect {
        left: left as usize,
        top: top as usize,
        right: right as usize,
        bottom: bottom as usize,
    })
}

#[cfg(test)]
mod tests;
