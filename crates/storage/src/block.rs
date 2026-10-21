// SPDX-License-Identifier: Apache-2.0
//! Enforce checked 512-byte logical sectors and safely handle unaligned ext2 metadata updates.
use crate::Error;

pub const SECTOR: usize = 512;

pub trait BlockDevice {
    fn sectors(&self) -> u64;
    fn read_sector(&mut self, lba: u64, bytes: &mut [u8; SECTOR]) -> Result<(), Error>;
    fn write_sector(&mut self, lba: u64, bytes: &[u8; SECTOR]) -> Result<(), Error>;
    /// Return success only if all preceding writes reached the medium's flush boundary.
    fn flush(&mut self) -> Result<(), Error>;
}

pub fn check(sectors: u64, start: u64, count: u64) -> Result<(), Error> {
    if start.checked_add(count).is_none_or(|end| end > sectors) {
        return Err(Error::OutOfRange);
    }
    Ok(())
}

pub fn read_at<D: BlockDevice>(
    device: &mut D,
    offset: u64,
    output: &mut [u8],
) -> Result<(), Error> {
    let length = u64::try_from(output.len()).map_err(|_| Error::Overflow)?;
    let end = offset.checked_add(length).ok_or(Error::Overflow)?;
    if end > device.sectors().checked_mul(SECTOR as u64).ok_or(Error::Overflow)? {
        return Err(Error::OutOfRange);
    }
    let mut filled = 0;
    while filled < output.len() {
        let pos = offset + filled as u64;
        let within = (pos % SECTOR as u64) as usize;
        let n = (SECTOR - within).min(output.len() - filled);
        let mut sector = [0; SECTOR];
        device.read_sector(pos / SECTOR as u64, &mut sector)?;
        output[filled..filled + n].copy_from_slice(&sector[within..within + n]);
        filled += n;
    }
    Ok(())
}

pub fn write_at<D: BlockDevice>(
    device: &mut D,
    offset: u64,
    input: &[u8],
) -> Result<(), Error> {
    let length = u64::try_from(input.len()).map_err(|_| Error::Overflow)?;
    let end = offset.checked_add(length).ok_or(Error::Overflow)?;
    if end > device.sectors().checked_mul(SECTOR as u64).ok_or(Error::Overflow)? {
        return Err(Error::OutOfRange);
    }
    let mut written = 0;
    while written < input.len() {
        let pos = offset + written as u64;
        let within = (pos % SECTOR as u64) as usize;
        let n = (SECTOR - within).min(input.len() - written);
        let mut sector = [0; SECTOR];
        if within != 0 || n != SECTOR {
            device.read_sector(pos / SECTOR as u64, &mut sector)?;
        }
        sector[within..within + n].copy_from_slice(&input[written..written + n]);
        device.write_sector(pos / SECTOR as u64, &sector)?;
        written += n;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Memory([u8; 1024]);
    impl BlockDevice for Memory {
        fn sectors(&self) -> u64 { 2 }
        fn read_sector(&mut self, lba: u64, bytes: &mut [u8; SECTOR]) -> Result<(), Error> {
            check(2, lba, 1)?;
            let i = lba as usize * SECTOR;
            bytes.copy_from_slice(&self.0[i..i + SECTOR]);
            Ok(())
        }
        fn write_sector(&mut self, lba: u64, bytes: &[u8; SECTOR]) -> Result<(), Error> {
            check(2, lba, 1)?;
            let i = lba as usize * SECTOR;
            self.0[i..i + SECTOR].copy_from_slice(bytes);
            Ok(())
        }
        fn flush(&mut self) -> Result<(), Error> { Ok(()) }
    }

    #[test]
    fn unaligned_roundtrip_and_bounds_do_not_write() {
        let mut device = Memory([0x55; 1024]);
        write_at(&mut device, 510, b"012345").unwrap();
        let mut bytes = [0; 6];
        read_at(&mut device, 510, &mut bytes).unwrap();
        assert_eq!(&bytes, b"012345");
        assert_eq!(device.0[509], 0x55);
        assert_eq!(device.0[516], 0x55);
        assert_eq!(write_at(&mut device, 1023, b"bad"), Err(Error::OutOfRange));
        assert_eq!(device.0[1023], 0x55);
        assert_eq!(read_at(&mut device, u64::MAX, &mut bytes), Err(Error::Overflow));
        assert!(check(u64::MAX, u64::MAX - 1, 2).is_err());
    }
}
