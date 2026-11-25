// SPDX-License-Identifier: Apache-2.0
//! Validate USB descriptors, HID reports, hub topology and xHCI register walks without DMA.
#![no_std]

pub mod hid;
pub mod topology;
pub mod xhci;

pub const MAX_INTERFACES: usize = 4;
pub const MAX_DESCRIPTOR: usize = 2048;
pub const MAX_PACKET: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Interface {
    pub number: u8,
    pub alternate: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoint: u8,
    pub packet: u16,
    pub interval: u8,
    pub report_bytes: u16,
}
#[derive(Clone, Copy, Debug)]
pub struct Configuration {
    pub value: u8,
    pub interfaces: [Interface; MAX_INTERFACES],
    pub count: usize,
}

/// Retain only alternate zero; validate every descriptor, including unsupported siblings.
/// The bounded HID/hub profile admits one interrupt IN endpoint per supported interface.
pub fn configuration(bytes: &[u8]) -> Result<Configuration, &'static str> {
    if bytes.len() < 9
        || bytes.len() > MAX_DESCRIPTOR
        || bytes[0] != 9
        || bytes[1] != 2
        || usize::from(u16::from_le_bytes([bytes[2], bytes[3]])) != bytes.len()
        || bytes[5] == 0
        || bytes[4] == 0
        || bytes[7] & 0x80 == 0
    {
        return Err("configuration-header");
    }
    let mut result = Configuration {
        value: bytes[5],
        interfaces: [Interface::default(); MAX_INTERFACES],
        count: 0,
    };
    let mut offset = 9;
    let mut current = None;
    let mut seen = [false; 256];
    let mut seen_alternates = [(0, 0); 32];
    let mut alternate_count = 0;
    let mut endpoint_bits = 0u32;
    let mut expected = 0;
    let mut actual = 0;
    while offset < bytes.len() {
        let header = bytes.get(offset..offset + 2).ok_or("descriptor-header")?;
        let length = usize::from(header[0]);
        if length < 2 {
            return Err("descriptor-length");
        }
        let d = bytes
            .get(offset..offset.checked_add(length).ok_or("descriptor-overflow")?)
            .ok_or("descriptor-truncated")?;
        match d[1] {
            2 | 1 => return Err("nested-device-configuration"),
            4 => {
                if length != 9 || actual != expected {
                    return Err("interface-length-endpoints");
                }
                let pair = (d[2], d[3]);
                if alternate_count == seen_alternates.len()
                    || seen_alternates[..alternate_count].contains(&pair)
                {
                    return Err("interface-duplicate-limit");
                }
                seen_alternates[alternate_count] = pair;
                alternate_count += 1;
                expected = d[4];
                actual = 0;
                current = None;
                if d[3] == 0 {
                    if seen[usize::from(d[2])] || result.count == MAX_INTERFACES {
                        return Err("interface-limit");
                    }
                    seen[usize::from(d[2])] = true;
                    current = Some(result.count);
                    result.interfaces[result.count] = Interface {
                        number: d[2],
                        alternate: d[3],
                        class: d[5],
                        subclass: d[6],
                        protocol: d[7],
                        ..Interface::default()
                    };
                    result.count += 1;
                }
            }
            5 => {
                if length < 7 || alternate_count == 0 || d[2] & 0x70 != 0 || d[2] & 15 == 0 {
                    return Err("endpoint-descriptor");
                }
                actual = actual.checked_add(1u8).ok_or("endpoint-count")?;
                if let Some(i) = current {
                    let bit = 1u32 << ((d[2] & 15) * 2 + (d[2] >> 7));
                    if endpoint_bits & bit != 0 {
                        return Err("endpoint-duplicate");
                    }
                    endpoint_bits |= bit;
                    let f = &mut result.interfaces[i];
                    if matches!(f.class, 3 | 9) && d[2] & 0x80 != 0 && d[3] & 3 == 3 {
                        let packet = u16::from_le_bytes([d[4], d[5]]);
                        if f.endpoint != 0 || packet == 0 || packet > MAX_PACKET as u16 || d[6] == 0
                        {
                            return Err("hid-endpoint-bounds");
                        }
                        f.endpoint = d[2];
                        f.packet = packet;
                        f.interval = d[6];
                    }
                }
            }
            0x21 => {
                if length < 6 || length != 6 + usize::from(d[5]) * 3 {
                    return Err("hid-class-length");
                }
                if let Some(i) = current {
                    let f = &mut result.interfaces[i];
                    if f.class == 3 {
                        for sub in d[6..].chunks_exact(3) {
                            if sub[0] == 0x22 {
                                if f.report_bytes != 0 {
                                    return Err("hid-report-duplicate");
                                }
                                let n = u16::from_le_bytes([sub[1], sub[2]]);
                                if n == 0 || usize::from(n) > MAX_DESCRIPTOR {
                                    return Err("hid-report-length");
                                }
                                f.report_bytes = n;
                            }
                        }
                    }
                } else if alternate_count == 0 {
                    return Err("hid-without-interface");
                }
            }
            _ => {}
        }
        offset += length;
    }
    if actual != expected || result.count != usize::from(bytes[4]) {
        return Err("configuration-interface-count");
    }
    Ok(result)
}

pub fn ep0_packet(speed: u8, byte: u8) -> Result<u16, &'static str> {
    match (speed, byte) {
        (2, 8) => Ok(8),
        (1, 8 | 16 | 32 | 64) => Ok(u16::from(byte)),
        (3, 64) => Ok(64),
        (4, 9) => Ok(512),
        _ => Err("ep0-packet"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONFIG: &[u8] = &[
        9, 2, 34, 0, 1, 1, 0, 128, 50, 9, 4, 0, 0, 1, 3, 1, 1, 0, 9, 33, 17, 1, 0, 1, 34, 63, 0, 7,
        5, 129, 3, 8, 0, 10,
    ];
    #[test]
    fn configuration_and_all_truncations() {
        let c = configuration(CONFIG).unwrap();
        assert_eq!(c.interfaces[0].endpoint, 129);
        assert_eq!(c.interfaces[0].report_bytes, 63);
        for n in 0..CONFIG.len() {
            assert!(configuration(&CONFIG[..n]).is_err());
        }
    }
    #[test]
    fn rejects_zero_length_counts_endpoint_zero_and_oversize() {
        for (at, value) in [(9, 0), (13, 2), (29, 128), (31, 65), (32, 1), (25, 0)] {
            let mut b = [0; 34];
            b.copy_from_slice(CONFIG);
            b[at] = value;
            assert!(configuration(&b).is_err(), "offset {at}");
        }
        assert_eq!(ep0_packet(1, 32), Ok(32));
        assert!(ep0_packet(3, 8).is_err());
        assert_eq!(ep0_packet(4, 9), Ok(512));
    }
    #[test]
    fn composite_and_alternate_interfaces_keep_independent_endpoints() {
        let mut bytes = [0u8; 59];
        bytes[..34].copy_from_slice(CONFIG);
        bytes[34..].copy_from_slice(&CONFIG[9..]);
        bytes[2] = 59;
        bytes[4] = 2;
        bytes[36] = 1;
        bytes[41] = 2;
        bytes[54] = 130;
        let c = configuration(&bytes).unwrap();
        assert_eq!(c.count, 2);
        assert_eq!(c.interfaces[1].number, 1);
        assert_eq!(c.interfaces[1].endpoint, 130);
        bytes[4] = 1;
        bytes[36] = 0;
        bytes[37] = 1;
        assert_eq!(configuration(&bytes).unwrap().count, 1);
        bytes[37] = 0;
        assert!(configuration(&bytes).is_err());
    }
}
