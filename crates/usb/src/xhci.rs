// SPDX-License-Identifier: Apache-2.0
//! Validate xHCI register spans, capability walks and explicit protocol speed-ID mappings.
//! The bootstrap ceiling is not a measured BAR size; callers must validate that separately.

pub const REGISTER_LIMIT: u32 = 256 * 1024;
const MAX_CAPABILITIES: usize = 256;

/// Map controller speed IDs to internal USB full/low/high/SuperSpeed classes.
/// Valid but unsupported rates remain zero; an explicit table has no implied defaults.
pub fn speed_map(major: u8, entries: &[u32]) -> Result<[u8; 16], &'static str> {
    if !matches!(major, 2 | 3) || entries.len() > 15 {
        return Err("protocol-speed-count");
    }
    let mut map = [0; 16];
    if entries.is_empty() {
        if major == 2 {
            map[1..4].copy_from_slice(&[1, 2, 3]);
        } else {
            map[4] = 4;
        }
        return Ok(map);
    }
    let mut seen = 0u16;
    let mut index = 0;
    while index < entries.len() {
        let word = entries[index];
        let id = (word & 15) as usize;
        let kind = (word >> 6) & 3;
        let mantissa = word >> 16;
        if id == 0 || mantissa == 0 || seen & (1 << id) != 0 || kind == 1 || kind == 3 {
            return Err("protocol-speed-entry");
        }
        seen |= 1 << id;
        if kind == 2 {
            let tx = *entries.get(index + 1).ok_or("protocol-speed-pair")?;
            if major != 3 || tx & 15 != id as u32 || (tx >> 6) & 3 != 3 || tx >> 16 == 0 {
                return Err("protocol-speed-pair");
            }
            index += 2;
            continue;
        }
        let rate =
            u64::from(mantissa) * [1, 1_000, 1_000_000, 1_000_000_000][((word >> 4) & 3) as usize];
        let link = (word >> 14) & 3;
        map[id] = match (major, link, rate) {
            (2, 0, 12_000_000) if word & 256 == 0 => 1,
            (2, 0, 1_500_000) if word & 256 == 0 => 2,
            (2, 0, 480_000_000) if word & 256 == 0 => 3,
            (3, 0, 5_000_000_000) => 4,
            _ => 0,
        };
        index += 1;
    }
    Ok(map)
}

/// Return the exclusive end of a nonempty DWORD-aligned register span.
pub fn register_end(offset: u32, bytes: u32, limit: u32) -> Result<u32, &'static str> {
    let end = offset.checked_add(bytes).ok_or("register-range")?;
    if offset & 3 != 0 || bytes < 4 || bytes & 3 != 0 || end > limit {
        return Err("register-range");
    }
    Ok(end)
}

/// Visit only checked headers; Next is a relative DWORD offset, zero terminates the chain.
/// The callback owns MMIO mapping and any capability-specific length/ownership checks.
pub fn extended_capabilities(
    mut offset: u32,
    limit: u32,
    mut visit: impl FnMut(u32) -> Result<u32, &'static str>,
) -> Result<(), &'static str> {
    for _ in 0..MAX_CAPABILITIES {
        if offset == 0 {
            return Ok(());
        }
        register_end(offset, 4, limit)?;
        let header = visit(offset)?;
        if header == u32::MAX || header & 255 == 0 {
            return Err("capability-header");
        }
        let next = (header >> 8) & 255;
        if next == 0 {
            return Ok(());
        }
        offset = offset.checked_add(next * 4).ok_or("capability-offset")?;
    }
    Err("capability-limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_speed_tables_preserve_ids_and_do_not_invent_defaults() {
        let map = speed_map(
            2,
            &[
                12 << 16 | 2 << 4 | 7,
                1500 << 16 | 1 << 4 | 9,
                480 << 16 | 2 << 4 | 3,
            ],
        )
        .unwrap();
        assert_eq!((map[7], map[9], map[3], map[1], map[2]), (1, 2, 3, 0, 0));
        assert_eq!(speed_map(2, &[]).unwrap()[1..4], [1, 2, 3]);
        let usb3 = speed_map(3, &[5 << 16 | 3 << 4 | 4, 10 << 16 | 1 << 14 | 3 << 4 | 5]).unwrap();
        assert_eq!((usb3[4], usb3[5]), (4, 0));
        assert_eq!(speed_map(2, &[12000 << 16 | 1 << 4 | 6]).unwrap()[6], 1);
    }

    #[test]
    fn speed_tables_reject_bad_entries_and_bound_asymmetric_pairs() {
        for words in [
            &[0][..],
            &[12 << 16][..],
            &[1][..],
            &[12 << 16 | 0x61][..],
            &[12 << 16 | 0xe1][..],
            &[12 << 16 | 0x21, 12 << 16 | 0x21][..],
        ] {
            assert!(speed_map(2, words).is_err());
        }
        let rx = 10 << 16 | 0xb5;
        let tx = 10 << 16 | 0xf5;
        assert!(speed_map(3, &[rx]).is_err());
        assert!(speed_map(3, &[rx, tx + 1]).is_err());
        assert_eq!(speed_map(3, &[rx, tx]).unwrap()[5], 0);
        assert!(speed_map(3, &[0; 16]).is_err());
        assert!(speed_map(1, &[]).is_err());
    }

    #[test]
    fn high_capabilities_reach_legacy_ownership_outside_the_first_page() {
        // Representative geometry: HCCPARAMS1 xECP=0x2000 DWORDs, legacy at 0x846c.
        let offsets = [0x8000, 0x8010, 0x840c, 0x846c];
        let headers = [2 | 4 << 8, 192 | 255 << 8, 193 | 24 << 8, 1];
        let mut count = 0;
        extended_capabilities(0x2000 * 4, REGISTER_LIMIT, |offset| {
            assert_eq!(offset, offsets[count]);
            let header = headers[count];
            count += 1;
            if header & 255 == 1 {
                assert_eq!(register_end(offset, 8, 4096), Err("register-range"));
                assert_eq!(register_end(offset, 8, 65536), Ok(0x8474));
            }
            Ok(header)
        })
        .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn rejects_bad_next_before_visiting_unmapped_registers() {
        let mut visits = 0;
        assert_eq!(
            extended_capabilities(REGISTER_LIMIT - 4, REGISTER_LIMIT, |_| {
                visits += 1;
                Ok(1 | 1 << 8)
            }),
            Err("register-range")
        );
        assert_eq!(visits, 1);
        for first in [3, REGISTER_LIMIT, u32::MAX - 3] {
            assert_eq!(
                extended_capabilities(first, REGISTER_LIMIT, |_| panic!("unchecked access")),
                Err("register-range")
            );
        }
    }

    #[test]
    fn terminates_empty_and_unknown_capabilities_but_rejects_invalid_headers() {
        assert_eq!(
            extended_capabilities(0, REGISTER_LIMIT, |_| panic!("empty")),
            Ok(())
        );
        assert_eq!(
            extended_capabilities(0x8000, REGISTER_LIMIT, |_| Ok(192)),
            Ok(())
        );
        for header in [0, u32::MAX] {
            assert_eq!(
                extended_capabilities(0x8000, REGISTER_LIMIT, |_| Ok(header)),
                Err("capability-header")
            );
        }
        assert_eq!(
            extended_capabilities(0x8000, REGISTER_LIMIT, |_| Err("bar-map")),
            Err("bar-map")
        );
    }

    #[test]
    fn limits_chain_work_even_when_every_next_is_inside_the_window() {
        let mut visits = 0;
        assert_eq!(
            extended_capabilities(0x8000, REGISTER_LIMIT, |_| {
                visits += 1;
                Ok(192 | 1 << 8)
            }),
            Err("capability-limit")
        );
        assert_eq!(visits, MAX_CAPABILITIES);
    }

    #[test]
    fn checks_cross_page_spans_and_the_later_measured_bar() {
        assert_eq!(register_end(4092, 8, REGISTER_LIMIT), Ok(4100));
        assert_eq!(
            register_end(REGISTER_LIMIT - 8, 8, REGISTER_LIMIT),
            Ok(REGISTER_LIMIT)
        );
        assert_eq!(
            register_end(REGISTER_LIMIT - 4, 8, REGISTER_LIMIT),
            Err("register-range")
        );
        // A controller-advertised bootstrap span must fit the subsequently measured aperture.
        assert_eq!(register_end(0, 0x8474, 0x10000), Ok(0x8474));
        assert_eq!(register_end(0, 0x8474, 0x8000), Err("register-range"));
        for (offset, bytes) in [(0, 0), (1, 4), (0, 5), (u32::MAX - 3, 8)] {
            assert_eq!(
                register_end(offset, bytes, REGISTER_LIMIT),
                Err("register-range")
            );
        }
    }
}
