// SPDX-License-Identifier: Apache-2.0
//! Admit only QEMU's legacy PM1a register and constant root _S5 package, without an AML interpreter.
//! This reference-platform helper rejects hardware-reduced ACPI, PM1b, methods and computed packages.
use crate::acpi;
type Result<T> = core::result::Result<T, &'static str>;
fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or("Truncated power field")?
            .try_into()
            .unwrap(),
    ))
}
pub fn qemu_register(bytes: &[u8]) -> Result<(u16, u64)> {
    let b = acpi::table(bytes, b"FACP")?;
    if b.get(10..16) != Some(b"BOCHS ") && b.get(10..16) != Some(b"QEMU  ") {
        return Err("Soft-off is restricted to QEMU firmware");
    }
    let port = u32_at(b, 64)?;
    if port == 0
        || port > 65534
        || u32_at(b, 68)? != 0
        || b.get(89) != Some(&2)
        || u32_at(b, 112)? & (1 << 20) != 0
    {
        return Err("Unsupported ACPI power registers");
    }
    let dsdt = if b.len() >= 148 {
        u64::from_le_bytes(b[140..148].try_into().unwrap())
    } else {
        0
    };
    let dsdt = if dsdt == 0 {
        u64::from(u32_at(b, 40)?)
    } else {
        dsdt
    };
    if dsdt == 0 {
        return Err("Missing DSDT");
    }
    if b.len() >= 184
        && b[172..184].iter().any(|v| *v != 0)
        && (b[172] != 1
            || b[173] != 16
            || b[174] != 0
            || !matches!(b[175], 0 | 2)
            || u64::from_le_bytes(b[176..184].try_into().unwrap()) != u64::from(port))
    {
        return Err("Unsupported extended PM1a register");
    }
    Ok((port as u16, dsdt))
}
pub fn qemu_s5(bytes: &[u8]) -> Result<u8> {
    let b = acpi::table(bytes, b"DSDT")?;
    let mut found = None;
    for at in 36..b.len() {
        let name = if b.get(at..at + 6) == Some(b"\x08\\_S5_") {
            at + 6
        } else if b.get(at..at + 5) == Some(b"\x08_S5_") {
            at + 5
        } else {
            continue;
        };
        if found.is_some() || b.get(name) != Some(&0x12) {
            return Err("Nonconstant or duplicate _S5");
        }
        let length = *b.get(name + 1).ok_or("Truncated _S5 package")? as usize;
        if length & 0xc0 != 0
            || length < 4
            || name + 1 + length > b.len()
            || b.get(name + 2) != Some(&4)
        {
            return Err("Unsupported _S5 package");
        }
        let mut pos = name + 3;
        let mut integer = || -> Result<u8> {
            let op = *b.get(pos).ok_or("Truncated sleep type")?;
            pos += 1;
            let v = match op {
                0 | 1 => op,
                0x0a => {
                    let v = *b.get(pos).ok_or("Truncated byte constant")?;
                    pos += 1;
                    v
                }
                _ => return Err("Computed sleep type"),
            };
            if v > 7 {
                return Err("Invalid sleep type");
            }
            Ok(v)
        };
        let a = integer()?;
        let other = integer()?;
        if a != other || integer()? != 0 || integer()? != 0 || pos != name + 1 + length {
            return Err("Unsupported _S5 constants");
        }
        found = Some(a);
    }
    found.ok_or("Missing constant _S5")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn checksum(bytes: &mut [u8]) {
        bytes[9] = 0;
        bytes[9] = 0u8.wrapping_sub(bytes.iter().fold(0u8, |sum, n| sum.wrapping_add(*n)));
    }
    #[test]
    fn only_reference_qemu_power_registers_are_admitted() {
        let mut b = [0; 244];
        b[..4].copy_from_slice(b"FACP");
        b[4..8].copy_from_slice(&244u32.to_le_bytes());
        b[10..16].copy_from_slice(b"BOCHS ");
        b[40..44].copy_from_slice(&0x1000u32.to_le_bytes());
        b[64..68].copy_from_slice(&0x604u32.to_le_bytes());
        b[89] = 2;
        checksum(&mut b);
        assert_eq!(qemu_register(&b), Ok((0x604, 0x1000)));
        for (at, mask) in [(10, 1), (68, 1), (89, 1), (114, 16), (172, 1)] {
            let mut bad = b;
            bad[at] ^= mask;
            checksum(&mut bad);
            assert!(qemu_register(&bad).is_err());
        }
        b[9] ^= 1;
        assert!(qemu_register(&b).is_err());
    }
    #[test]
    fn constant_sleep_package_rejects_computation_truncation_and_bad_values() {
        let mut b = [0; 49];
        b[..4].copy_from_slice(b"DSDT");
        b[4..8].copy_from_slice(&49u32.to_le_bytes());
        b[36..].copy_from_slice(b"\x08\\_S5_\x12\x06\x04\x00\x00\x00\x00");
        checksum(&mut b);
        assert_eq!(qemu_s5(&b), Ok(0));
        for (at, v) in [
            (36, 0x14),
            (42, 0x14),
            (43, 0xff),
            (44, 2),
            (45, 8),
            (46, 1),
            (47, 1),
        ] {
            let mut bad = b;
            bad[at] = v;
            checksum(&mut bad);
            assert!(qemu_s5(&bad).is_err());
        }
        for len in 0..b.len() {
            assert!(qemu_s5(&b[..len]).is_err());
        }
    }
}
