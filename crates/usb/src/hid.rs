// SPDX-License-Identifier: Apache-2.0
//! Decode transactional Boot reports and bounded HID short-item Report layouts.
//! No text, repeat, focus, allocation, transport or device-lifetime policy belongs here.
use crate::MAX_PACKET;

pub const MAX_REPORTS: usize = 8;
const MAX_FIELDS: usize = 48;
const MAX_USAGES: usize = 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sample {
    pub keys: [u64; 4],
    pub buttons: u8,
    pub x: i16,
    pub y: i16,
    pub wheel: i16,
}
impl Sample {
    pub fn key(&mut self, usage: u32) -> Result<(), &'static str> {
        if (1..=3).contains(&usage) {
            return Err("keyboard-rollover");
        }
        if usage < 256 && usage != 0 {
            self.keys[usage as usize / 64] |= 1 << (usage % 64);
        }
        Ok(())
    }
}
pub fn boot_keyboard(bytes: &[u8]) -> Result<Sample, &'static str> {
    if bytes.len() != 8 || bytes[1] != 0 {
        return Err("boot-keyboard-length-reserved");
    }
    let mut sample = Sample::default();
    for bit in 0..8 {
        if bytes[0] & (1 << bit) != 0 {
            sample.key(224 + bit)?;
        }
    }
    for &usage in &bytes[2..] {
        sample.key(u32::from(usage))?;
    }
    Ok(sample)
}
pub fn boot_mouse(bytes: &[u8]) -> Result<Sample, &'static str> {
    if bytes.len() < 3 || bytes.len() > MAX_PACKET {
        return Err("boot-mouse-length");
    }
    Ok(Sample {
        buttons: bytes[0] & 7,
        x: i16::from(bytes[1] as i8),
        y: i16::from(bytes[2] as i8),
        ..Sample::default()
    })
}

#[derive(Clone, Copy, Default)]
struct Globals {
    page: u32,
    min: i64,
    max: i64,
    size: u32,
    count: u32,
    id: u8,
}
#[derive(Clone, Copy, Default)]
struct Locals {
    usages: [u32; MAX_USAGES],
    count: usize,
    min: Option<u32>,
    max: Option<u32>,
}
impl Locals {
    fn usage(&self, index: u32) -> Option<u32> {
        if self.count > 0 {
            self.usages
                .get((index as usize).min(self.count - 1))
                .copied()
        } else {
            let min = self.min?;
            let max = self.max?;
            Some(min.checked_add(index)?.min(max))
        }
    }
    fn validate(&self) -> Result<(), &'static str> {
        if self.min.is_some() != self.max.is_some()
            || self
                .min
                .zip(self.max)
                .is_some_and(|(a, b)| a > b || a >> 16 != b >> 16)
            || self.count > 0 && self.min.is_some()
        {
            return Err("hid-usage-range");
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Default)]
struct Field {
    report: usize,
    offset: u32,
    globals: Globals,
    locals: Locals,
    flags: u32,
}
#[derive(Clone, Copy, Default)]
struct Report {
    id: u8,
    bits: [u32; 3],
    usable: bool,
}
/// At most eight IDs, 48 Input fields, 16 explicit usages per Main item, four pushed
/// globals, eight collections and 64 bytes per wire report. Unknown fields retain offsets.
pub struct Layout {
    fields: [Field; MAX_FIELDS],
    count: usize,
    reports: [Report; MAX_REPORTS],
    report_count: usize,
    ids: bool,
}

fn unsigned(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .enumerate()
        .fold(0, |v, (i, b)| v | u32::from(*b) << (i * 8))
}
fn signed(bytes: &[u8]) -> i32 {
    let value = unsigned(bytes);
    if bytes.is_empty() {
        0
    } else {
        ((value << (32 - bytes.len() * 8)) as i32) >> (32 - bytes.len() * 8)
    }
}
fn usage(value: u32, length: usize, page: u32) -> Result<u32, &'static str> {
    if page > 65535 {
        return Err("hid-usage-page");
    }
    Ok(if length == 4 {
        value
    } else {
        page << 16 | value
    })
}
impl Layout {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.is_empty() || bytes.len() > crate::MAX_DESCRIPTOR {
            return Err("hid-descriptor-size");
        }
        let mut result = Self {
            fields: [Field::default(); MAX_FIELDS],
            count: 0,
            reports: [Report::default(); MAX_REPORTS],
            report_count: 1,
            ids: false,
        };
        let mut g = Globals::default();
        let mut local = Locals::default();
        let mut stack = [Globals::default(); 4];
        let mut pushed = 0usize;
        let mut depth = 0usize;
        let mut offset = 0;
        while offset < bytes.len() {
            let prefix = bytes[offset];
            offset += 1;
            if prefix == 0xfe {
                return Err("hid-long-item-unsupported");
            }
            let length = match prefix & 3 {
                3 => 4,
                n => usize::from(n),
            };
            let data = bytes
                .get(offset..offset + length)
                .ok_or("hid-item-truncated")?;
            offset += length;
            let value = unsigned(data);
            let tag = prefix >> 4;
            match (prefix >> 2) & 3 {
                1 => match tag {
                    0 => {
                        if value > 65535 {
                            return Err("hid-usage-page");
                        }
                        g.page = value;
                    }
                    1 => g.min = i64::from(signed(data)),
                    2 => {
                        g.max = if g.min < 0 {
                            i64::from(signed(data))
                        } else {
                            i64::from(value)
                        };
                    }
                    3..=6 => {} // Physical range, unit and exponent do not change Input bit layout.
                    7 => g.size = value,
                    8 => {
                        if value == 0
                            || value > 255
                            || result.reports[0].bits.iter().any(|n| *n != 0) && !result.ids
                        {
                            return Err("hid-report-id");
                        }
                        if !result.ids {
                            result.ids = true;
                            result.report_count = 0;
                        }
                        g.id = value as u8;
                    }
                    9 => g.count = value,
                    10 => {
                        if length != 0 || pushed == stack.len() {
                            return Err("hid-push-limit");
                        }
                        stack[pushed] = g;
                        pushed += 1;
                    }
                    11 => {
                        if length != 0 || pushed == 0 {
                            return Err("hid-pop-underflow");
                        }
                        pushed -= 1;
                        g = stack[pushed];
                    }
                    _ => return Err("hid-global-unsupported"),
                },
                2 => match tag {
                    0 => {
                        if local.count == MAX_USAGES {
                            return Err("hid-usage-limit");
                        }
                        local.usages[local.count] = usage(value, length, g.page)?;
                        local.count += 1;
                    }
                    1 => local.min = Some(usage(value, length, g.page)?),
                    2 => local.max = Some(usage(value, length, g.page)?),
                    3..=5 | 7..=9 => {} // Designator/string metadata carries no input state.
                    _ => return Err("hid-local-unsupported"),
                },
                0 => {
                    local.validate()?;
                    match tag {
                        10 => {
                            if length != 1 || depth == 8 {
                                return Err("hid-collection-limit");
                            }
                            depth += 1;
                        }
                        12 => {
                            if length != 0 || depth == 0 {
                                return Err("hid-collection-underflow");
                            }
                            depth -= 1;
                        }
                        8 | 9 | 11 => {
                            if depth == 0
                                || g.size == 0
                                || g.size > 32
                                || g.count == 0
                                || g.count > 256
                                || g.min > g.max
                                || result.ids && g.id == 0
                            {
                                return Err("hid-field-bounds");
                            }
                            let report = if let Some(i) = result.reports[..result.report_count]
                                .iter()
                                .position(|r| r.id == g.id)
                            {
                                i
                            } else {
                                if result.report_count == MAX_REPORTS {
                                    return Err("hid-report-limit");
                                }
                                let i = result.report_count;
                                result.reports[i].id = g.id;
                                result.report_count += 1;
                                i
                            };
                            let kind = match tag {
                                8 => 0,
                                9 => 1,
                                _ => 2,
                            };
                            let bits = &mut result.reports[report].bits[kind];
                            let start = *bits;
                            *bits = bits
                                .checked_add(
                                    g.size.checked_mul(g.count).ok_or("hid-field-overflow")?,
                                )
                                .ok_or("hid-offset-overflow")?;
                            if *bits > ((MAX_PACKET - usize::from(result.ids)) * 8) as u32 {
                                return Err("hid-report-size");
                            }
                            if tag == 8 && value & 1 == 0 {
                                if result.count == MAX_FIELDS {
                                    return Err("hid-field-limit");
                                }
                                let mut usable = false;
                                for i in 0..g.count {
                                    if let Some(u) = local.usage(i) {
                                        usable |= u >> 16 == 7
                                            || u >> 16 == 9
                                                && value & 2 != 0
                                                && (1..=8).contains(&(u & 65535))
                                            || value & 6 == 6
                                                && matches!(u, 0x10030 | 0x10031 | 0x10038)
                                                && g.min >= i64::from(i16::MIN)
                                                && g.max <= i64::from(i16::MAX);
                                    }
                                }
                                result.reports[report].usable |= usable;
                                result.fields[result.count] = Field {
                                    report,
                                    offset: start,
                                    globals: g,
                                    locals: local,
                                    flags: value,
                                };
                                result.count += 1;
                            }
                        }
                        _ => return Err("hid-main-unsupported"),
                    }
                    local = Locals::default();
                }
                _ => return Err("hid-reserved-item"),
            }
        }
        if depth != 0
            || pushed != 0
            || !result.reports[..result.report_count]
                .iter()
                .any(|r| r.usable)
        {
            return Err("hid-incomplete-layout");
        }
        Ok(result)
    }
    /// A Boot interface switches only when its primary keyboard or relative X/Y
    /// function survives Report mode; a buttons-only layout must not replace a mouse.
    pub fn supports_boot(&self, protocol: u8) -> bool {
        let mut keyboard = false;
        let mut axes = 0;
        for f in &self.fields[..self.count] {
            for i in 0..f.globals.count {
                if let Some(u) = f.locals.usage(i) {
                    keyboard |= u >> 16 == 7;
                    if f.flags & 6 == 6
                        && f.globals.min >= i64::from(i16::MIN)
                        && f.globals.max <= i64::from(i16::MAX)
                    {
                        axes |= match u {
                            0x10030 => 1,
                            0x10031 => 2,
                            _ => 0,
                        };
                    }
                }
            }
        }
        match protocol {
            1 => keyboard,
            2 => axes == 3,
            _ => false,
        }
    }

    pub fn max_input_bytes(&self) -> usize {
        self.reports[..self.report_count]
            .iter()
            .map(|r| r.bits[0].div_ceil(8) as usize + usize::from(self.ids))
            .max()
            .unwrap_or(0)
    }
    pub fn decode(&self, bytes: &[u8]) -> Result<(usize, Sample), &'static str> {
        if bytes.is_empty() || bytes.len() > MAX_PACKET {
            return Err("hid-packet-size");
        }
        let (report, data) = if self.ids {
            (
                self.reports[..self.report_count]
                    .iter()
                    .position(|r| r.id == bytes[0])
                    .ok_or("hid-unknown-report")?,
                &bytes[1..],
            )
        } else {
            (0, bytes)
        };
        let r = &self.reports[report];
        if !r.usable || data.len() < r.bits[0].div_ceil(8) as usize {
            return Err("hid-packet-truncated");
        }
        let mut sample = Sample::default();
        for f in &self.fields[..self.count] {
            if f.report != report {
                continue;
            }
            for i in 0..f.globals.count {
                let bit = f.offset + i * f.globals.size;
                let mut raw = 0u32;
                for n in 0..f.globals.size {
                    raw |= u32::from((data[((bit + n) / 8) as usize] >> ((bit + n) % 8)) & 1) << n;
                }
                let value = if f.globals.min < 0 {
                    i64::from(((raw << (32 - f.globals.size)) as i32) >> (32 - f.globals.size))
                } else {
                    i64::from(raw)
                };
                if value < f.globals.min || value > f.globals.max {
                    if f.flags & 64 != 0 {
                        continue;
                    } // Null-state input has no active usage.
                    return Err("hid-logical-value");
                }
                let variable = f.flags & 2 != 0;
                let u = if variable {
                    f.locals.usage(i)
                } else {
                    let index =
                        u32::try_from(value - f.globals.min).map_err(|_| "hid-array-index")?;
                    // Array selector outside its declared usage domain is invalid, not a repeated last usage.
                    if f.locals.count > 0 && index as usize >= f.locals.count
                        || f.locals
                            .min
                            .zip(f.locals.max)
                            .is_some_and(|(a, b)| index > b - a)
                    {
                        return Err("hid-array-usage");
                    }
                    f.locals.usage(index)
                };
                let Some(u) = u else {
                    continue;
                };
                match u >> 16 {
                    7 if !variable || value != 0 => sample.key(u & 65535)?,
                    9 if variable && value != 0 && (1..=8).contains(&(u & 65535)) => {
                        sample.buttons |= 1 << ((u & 65535) - 1)
                    }
                    1 if variable && f.flags & 4 != 0 => {
                        let axis = match u & 65535 {
                            0x30 => &mut sample.x,
                            0x31 => &mut sample.y,
                            0x38 => &mut sample.wheel,
                            _ => continue,
                        };
                        *axis = axis
                            .checked_add(i16::try_from(value).map_err(|_| "hid-axis-range")?)
                            .ok_or("hid-axis-overflow")?;
                    }
                    _ => {}
                }
            }
        }
        Ok((report, sample))
    }
}

/// Transactional per-interface state for independent Report IDs. Motion belongs only
/// to the completing report; held keys/buttons aggregate across every report in this interface.
#[derive(Default)]
pub struct State {
    reports: [Sample; MAX_REPORTS],
}
impl State {
    pub fn accept(&mut self, index: usize, sample: Sample) -> Result<Sample, &'static str> {
        *self
            .reports
            .get_mut(index)
            .ok_or("hid-state-report-index")? = sample;
        let mut result = sample;
        result.keys = [0; 4];
        result.buttons = 0;
        for state in &self.reports {
            for (out, held) in result.keys.iter_mut().zip(state.keys) {
                *out |= held;
            }
            result.buttons |= state.buttons;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MOUSE: &[u8] = &[
        5, 1, 9, 2, 0xa1, 1, 5, 9, 0x19, 1, 0x29, 5, 0x15, 0, 0x25, 1, 0x75, 1, 0x95, 5, 0x81, 2,
        0x75, 3, 0x95, 1, 0x81, 1, 5, 1, 9, 0x30, 9, 0x31, 9, 0x38, 0x15, 0x81, 0x25, 0x7f, 0x75,
        8, 0x95, 3, 0x81, 6, 0xc0,
    ];
    #[test]
    fn boot_rollover_reserved_truncation_and_release() {
        let s = boot_keyboard(&[2, 0, 4, 5, 6, 0, 0, 0]).unwrap();
        assert_ne!(s.keys[3] & (1 << 33), 0);
        assert_eq!(s.keys[0], 0x70);
        assert!(boot_keyboard(&[0, 0, 1, 4, 0, 0, 0, 0]).is_err());
        assert!(boot_keyboard(&[0; 7]).is_err());
        assert!(boot_keyboard(&[0, 1, 0, 0, 0, 0, 0, 0]).is_err());
        assert_eq!(boot_keyboard(&[0; 8]).unwrap(), Sample::default());
        assert_eq!(
            boot_mouse(&[7, 255, 128, 99]).unwrap(),
            Sample {
                buttons: 7,
                x: -1,
                y: -128,
                ..Sample::default()
            }
        );
    }
    #[test]
    fn signed_wheel_buttons_and_transactional_truncation() {
        let layout = Layout::parse(MOUSE).unwrap();
        assert_eq!(layout.max_input_bytes(), 4);
        assert_eq!(
            layout.decode(&[31, 0xff, 3, 0xfe]).unwrap().1,
            Sample {
                buttons: 31,
                x: -1,
                y: 3,
                wheel: -2,
                ..Sample::default()
            }
        );
        for n in 0..4 {
            assert!(layout.decode(&[0; 4][..n]).is_err());
        }
        for n in 0..MOUSE.len() {
            assert!(Layout::parse(&MOUSE[..n]).is_err());
        }
    }
    #[test]
    fn ids_push_pop_unknown_padding_and_cross_byte_signed_axis() {
        let descriptor = &[
            5, 1, 9, 2, 0xa1, 1, 0x85, 1, 0x15, 0, 0x25, 1, 0x75, 3, 0x95, 1, 0x81, 1, 0xa4, 9,
            0x30, 0x16, 0, 0xf8, 0x26, 0xff, 7, 0x75, 12, 0x81, 6, 0xb4, 0x85, 2, 5, 7, 0x19, 4,
            0x29, 11, 0x75, 1, 0x95, 8, 0x81, 2, 0xc0,
        ];
        let l = Layout::parse(descriptor).unwrap();
        assert_eq!(l.decode(&[1, 0xf8, 0x7f]).unwrap().1.x, -1);
        assert_eq!(l.decode(&[2, 3]).unwrap().1.keys[0], (1 << 4) | (1 << 5));
        assert!(l.decode(&[0, 0, 0]).is_err());
        assert!(l.decode(&[1, 0]).is_err());
        let mut bad = [0u8; 7];
        bad.copy_from_slice(&[5, 1, 9, 2, 0xa1, 1, 0xb4]);
        assert!(Layout::parse(&bad).is_err());
    }
    #[test]
    fn arrays_unknown_fields_overflow_and_unbalanced_descriptors() {
        let d = &[
            5, 1, 9, 6, 0xa1, 1, 5, 7, 0x19, 0, 0x29, 101, 0x15, 0, 0x25, 101, 0x75, 8, 0x95, 6,
            0x81, 0, 0xc0,
        ];
        let l = Layout::parse(d).unwrap();
        assert_eq!(l.decode(&[4, 5, 0, 0, 0, 0]).unwrap().1.keys[0], 48);
        assert!(l.decode(&[4, 1, 0, 0, 0, 0]).is_err());
        assert!(l.decode(&[102, 0, 0, 0, 0, 0]).is_err());
        for bad in [
            &[0xc0][..],
            &[0xfe, 0, 0],
            &[0x85, 0],
            &[0x75, 33],
            &[0xa1, 1, 0xa4, 0xc0],
        ] {
            assert!(Layout::parse(bad).is_err());
        }
        let mut huge = [0; 64];
        huge[..MOUSE.len()].copy_from_slice(MOUSE);
        huge[17] = 255;
        assert!(Layout::parse(&huge[..MOUSE.len()]).is_err());
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;
    #[test]
    fn report_ids_preserve_sibling_keys_buttons_and_only_current_motion() {
        let mut state = State::default();
        let mut a = Sample {
            buttons: 1,
            wheel: 3,
            ..Sample::default()
        };
        a.key(225).unwrap();
        state.accept(0, a).unwrap();
        let mut b = Sample {
            buttons: 8,
            x: -10,
            ..Sample::default()
        };
        b.key(225).unwrap();
        b.key(4).unwrap();
        let mixed = state.accept(1, b).unwrap();
        assert_eq!(mixed.buttons, 9);
        assert_eq!(mixed.wheel, 0);
        let released = state.accept(0, Sample::default()).unwrap();
        assert_eq!(released.keys, b.keys);
        assert_eq!(released.buttons, 8);
        assert_eq!(released.x, 0);
        assert!(state.accept(MAX_REPORTS, Sample::default()).is_err());
        assert_eq!(state.accept(0, Sample::default()).unwrap(), released);
    }
    #[test]
    fn bounds_nesting_ids_and_counts_fail_closed() {
        let mut depth = [0u8; 30];
        depth[..6].copy_from_slice(&[5, 1, 9, 6, 0xa1, 1]);
        for pair in depth[6..24].chunks_exact_mut(2) {
            pair.copy_from_slice(&[0xa1, 1]);
        }
        assert!(Layout::parse(&depth[..24]).is_err());
        let mut push = [0xa4; 5];
        assert!(Layout::parse(&push).is_err());
        push[4] = 0xb4;
        assert!(Layout::parse(&push).is_err());
        for size in [0, 33, 255] {
            let d = [
                5, 1, 9, 6, 0xa1, 1, 5, 7, 0x19, 0, 0x29, 101, 0x15, 0, 0x25, 101, 0x75, size,
                0x95, 6, 0x81, 0, 0xc0,
            ];
            assert!(Layout::parse(&d).is_err());
        }
    }
}

#[cfg(test)]
mod unknown_field_tests {
    use super::*;
    #[test]
    fn vendor_input_advances_offset_before_unaligned_keyboard_array() {
        let d = &[
            5, 1, 9, 6, 0xa1, 1, 6, 0, 0xff, 9, 1, 0x15, 0, 0x25, 31, 0x75, 5, 0x95, 1, 0x81, 2, 5,
            7, 0x19, 0, 0x29, 101, 0x25, 101, 0x75, 8, 0x81, 0, 0xc0,
        ];
        let l = Layout::parse(d).unwrap();
        assert_eq!(l.max_input_bytes(), 2);
        assert_eq!(l.decode(&[128, 0]).unwrap().1.keys[0], 16);
        assert!(l.decode(&[128]).is_err());
    }
}

#[cfg(test)]
mod unsigned_field_tests {
    use super::*;
    #[test]
    fn unsigned_32bit_vendor_field_keeps_offset_and_unsigned_logical_bounds() {
        let d = &[
            5, 1, 9, 6, 0xa1, 1, 6, 0, 0xff, 9, 1, 0x15, 0, 0x27, 255, 255, 255, 255, 0x75, 32,
            0x95, 1, 0x81, 2, 5, 7, 0x19, 0, 0x29, 101, 0x25, 101, 0x75, 8, 0x81, 0, 0xc0,
        ];
        let l = Layout::parse(d).unwrap();
        assert_eq!(l.max_input_bytes(), 5);
        assert_eq!(l.decode(&[255, 255, 255, 255, 4]).unwrap().1.keys[0], 16);
        assert!(l.decode(&[255, 255, 255, 255]).is_err());
    }
}

#[cfg(test)]
mod usability_tests {
    use super::*;
    #[test]
    fn relative_array_is_not_a_usable_pointer_variable() {
        let d = &[
            5, 1, 9, 2, 0xa1, 1, 9, 0x30, 0x15, 0, 0x25, 127, 0x75, 8, 0x95, 1, 0x81, 4, 0xc0,
        ];
        assert!(Layout::parse(d).is_err());
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    #[test]
    fn buttons_only_report_does_not_replace_boot_relative_mouse() {
        let d = &[
            5, 1, 9, 2, 0xa1, 1, 5, 9, 0x19, 1, 0x29, 3, 0x15, 0, 0x25, 1, 0x75, 1, 0x95, 3, 0x81,
            2, 0xc0,
        ];
        let l = Layout::parse(d).unwrap();
        assert!(!l.supports_boot(2));
        assert!(!l.supports_boot(1));
        assert_eq!(l.decode(&[7]).unwrap().1.buttons, 7);
    }
}
