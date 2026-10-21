// SPDX-License-Identifier: Apache-2.0
//! ACPI 6.6, sections 5.2.5, 5.2.8, 5.2.9 and 5.2.12.
//! Decode bytes instead of creating references to packed firmware structures.

pub const MAX_TABLE: usize = 1024 * 1024;
pub const MAX_ROOT_ENTRIES: usize = 128;

type Result<T> = core::result::Result<T, &'static str>;

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("Truncated ACPI u32")?
            .try_into()
            .unwrap(),
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or("Truncated ACPI u64")?
            .try_into()
            .unwrap(),
    ))
}

fn checksum(bytes: &[u8]) -> Result<()> {
    if bytes.iter().fold(0_u8, |sum, b| sum.wrapping_add(*b)) == 0 {
        Ok(())
    } else {
        Err("Invalid ACPI checksum")
    }
}

pub fn rsdp_length(bytes: &[u8]) -> Result<usize> {
    if bytes.len() < 20 || &bytes[..8] != b"RSD PTR " {
        return Err("Invalid RSDP signature or length");
    }
    checksum(&bytes[..20])?;
    if bytes[15] < 2 {
        return Ok(20);
    }
    let length = u32_at(bytes, 20)? as usize;
    if !(36..=4096).contains(&length) {
        return Err("Unsupported RSDP length");
    }
    Ok(length)
}

pub fn root_address(bytes: &[u8]) -> Result<(u64, usize)> {
    let length = rsdp_length(bytes)?;
    if length > bytes.len() {
        return Err("Truncated extended RSDP");
    }
    if length > 20 {
        checksum(&bytes[..length])?;
        let xsdt = u64_at(bytes, 24)?;
        if xsdt != 0 {
            return Ok((xsdt, 8));
        }
    }
    let rsdt = u64::from(u32_at(bytes, 16)?);
    if rsdt == 0 {
        Err("Missing ACPI root table")
    } else {
        Ok((rsdt, 4))
    }
}

pub fn table_length(header: &[u8]) -> Result<usize> {
    if header.len() < 36 {
        return Err("Truncated ACPI header");
    }
    let length = u32_at(header, 4)? as usize;
    if !(36..=MAX_TABLE).contains(&length) {
        return Err("Invalid ACPI table length");
    }
    Ok(length)
}

pub fn table<'a>(bytes: &'a [u8], signature: &[u8; 4]) -> Result<&'a [u8]> {
    let length = table_length(bytes)?;
    if bytes.get(..4) != Some(signature) || length > bytes.len() {
        return Err("Wrong ACPI signature or truncated table");
    }
    checksum(&bytes[..length])?;
    Ok(&bytes[..length])
}

pub fn root_entries(
    bytes: &[u8],
    width: usize,
    out: &mut [u64; MAX_ROOT_ENTRIES],
) -> Result<usize> {
    let signature = match width {
        4 => b"RSDT",
        8 => b"XSDT",
        _ => return Err("Invalid ACPI root entry width"),
    };
    let bytes = table(bytes, signature)?;
    let payload = &bytes[36..];
    if payload.len() % width != 0 || payload.len() / width > out.len() {
        return Err("Invalid or oversized ACPI root entry list");
    }
    for (index, value) in payload.chunks_exact(width).enumerate() {
        out[index] = if width == 8 {
            u64_at(value, 0)?
        } else {
            u64::from(u32_at(value, 0)?)
        };
        if out[index] == 0 {
            return Err("Null ACPI root entry");
        }
    }
    Ok(payload.len() / width)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoApic {
    pub id: u8,
    pub address: u64,
    pub gsi_base: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Madt {
    pub lapic: u64,
    pub ioapics: [Option<IoApic>; 4],
    pub enabled_cpus: usize,
}

pub fn madt(bytes: &[u8]) -> Result<Madt> {
    let bytes = table(bytes, b"APIC")?;
    if bytes.len() < 44 {
        return Err("Truncated MADT");
    }
    let mut result = Madt {
        lapic: u64::from(u32_at(bytes, 36)?),
        ioapics: [None; 4],
        enabled_cpus: 0,
    };
    let mut offset = 44;
    let mut io_count = 0;
    let mut lapic_override = false;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 2)
            .ok_or("Truncated MADT record")?;
        let length = usize::from(header[1]);
        if length < 2 {
            return Err("Non-progressing MADT record");
        }
        let record = bytes
            .get(offset..offset + length)
            .ok_or("MADT record exceeds table")?;
        match record[0] {
            0 => {
                if length != 8 {
                    return Err("Invalid local APIC record");
                }
                result.enabled_cpus += usize::from(u32_at(record, 4)? & 1 != 0);
            }
            1 => {
                if length != 12 || io_count == result.ioapics.len() {
                    return Err("Invalid or excessive I/O APIC records");
                }
                result.ioapics[io_count] = Some(IoApic {
                    id: record[2],
                    address: u64::from(u32_at(record, 4)?),
                    gsi_base: u32_at(record, 8)?,
                });
                io_count += 1;
            }
            5 => {
                if length != 12 || lapic_override {
                    return Err("Invalid APIC address override");
                }
                result.lapic = u64_at(record, 4)?;
                lapic_override = true;
            }
            9 => return Err("x2APIC topology is not supported by the BSP baseline"),
            _ => {} // External interrupt overrides/NMIs remain masked in M2.
        }
        offset += length;
    }
    if result.enabled_cpus == 0 || result.lapic == 0 || result.lapic & 0xfff != 0 || io_count == 0 {
        return Err("Incomplete xAPIC topology");
    }
    for (i, item) in result.ioapics.iter().flatten().enumerate() {
        if item.address == 0 || item.address & 0xfff != 0 {
            return Err("Invalid I/O APIC address");
        }
        if result
            .ioapics
            .iter()
            .flatten()
            .take(i)
            .any(|other| other.address == item.address || other.id == item.id)
        {
            return Err("Duplicate I/O APIC");
        }
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PmTimer {
    pub port: u16,
    pub mask: u32,
}

pub fn pm_timer(bytes: &[u8]) -> Result<PmTimer> {
    let bytes = table(bytes, b"FACP")?;
    if bytes.len() < 116 || bytes[91] != 4 {
        return Err("Missing ACPI PM timer");
    }
    let mut port = u64::from(u32_at(bytes, 76)?);
    if bytes.len() >= 220 {
        let extended = u64_at(bytes, 212)?;
        if extended != 0 {
            if bytes[208] != 1 {
                return Err("Only System I/O PM timers are supported");
            }
            port = extended;
        }
    }
    if port == 0 || port > u64::from(u16::MAX - 3) {
        return Err("Invalid PM timer I/O range");
    }
    Ok(PmTimer {
        port: port as u16,
        mask: if u32_at(bytes, 112)? & (1 << 8) == 0 {
            0x00ff_ffff
        } else {
            u32::MAX
        },
    })
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{vec, vec::Vec};

    fn fix(bytes: &mut [u8], index: usize) {
        bytes[index] = 0;
        bytes[index] = 0_u8.wrapping_sub(bytes.iter().fold(0_u8, |s, b| s.wrapping_add(*b)));
    }
    fn sdt(sig: &[u8; 4], length: usize) -> Vec<u8> {
        let mut b = vec![0; length];
        b[..4].copy_from_slice(sig);
        b[4..8].copy_from_slice(&(length as u32).to_le_bytes());
        b
    }
    #[test]
    fn rejects_corruption_truncation_and_oversized_tables() {
        let mut b = sdt(b"XSDT", 44);
        b[36..44].copy_from_slice(&0x1234_u64.to_le_bytes());
        fix(&mut b, 9);
        let mut out = [0; MAX_ROOT_ENTRIES];
        assert_eq!(root_entries(&b, 8, &mut out), Ok(1));
        assert_eq!(out[0], 0x1234);
        assert!(root_entries(&b[..43], 8, &mut out).is_err());
        b[40] ^= 1;
        assert!(root_entries(&b, 8, &mut out).is_err());
        b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(table_length(&b).is_err());
    }
    #[test]
    fn validates_both_rsdp_checksums() {
        let mut b = vec![0; 36];
        b[..8].copy_from_slice(b"RSD PTR ");
        b[15] = 2;
        b[20..24].copy_from_slice(&36_u32.to_le_bytes());
        b[24..32].copy_from_slice(&0x1234_u64.to_le_bytes());
        fix(&mut b[..20], 8);
        fix(&mut b, 32);
        assert_eq!(root_address(&b), Ok((0x1234, 8)));
        b[31] ^= 1;
        assert!(root_address(&b).is_err());
        assert!(root_address(&b[..20]).is_err());
    }
    #[test]
    fn rejects_madt_zero_length_and_truncated_records() {
        let mut b = sdt(b"APIC", 46);
        fix(&mut b, 9);
        assert_eq!(madt(&b), Err("Non-progressing MADT record"));
        b[45] = 8;
        fix(&mut b, 9);
        assert_eq!(madt(&b), Err("MADT record exceeds table"));
    }
    #[test]
    fn extended_pm_timer_takes_precedence_and_rejects_unsupported_space() {
        let mut b = sdt(b"FACP", 220);
        b[76..80].copy_from_slice(&0x408_u32.to_le_bytes());
        b[91] = 4;
        b[208] = 1;
        b[212..220].copy_from_slice(&0x508_u64.to_le_bytes());
        fix(&mut b, 9);
        assert_eq!(
            pm_timer(&b),
            Ok(PmTimer {
                port: 0x508,
                mask: 0x00ff_ffff
            })
        );
        b[208] = 0;
        fix(&mut b, 9);
        assert!(pm_timer(&b).is_err());
    }
}
