// SPDX-License-Identifier: Apache-2.0
//! Encode a bounded native argc/argv/envp startup block without host pointers.
use crate::Error;
use anova_abi::UserAddr;

pub fn build(args: &[&str], page: &mut [u8; 4096], top: u64) -> Result<u64, Error> {
    if args.is_empty()
        || args.len() > 16
        || top & 4095 != 0
        || top
            .checked_sub(4096)
            .and_then(|base| UserAddr(base).checked_end(4096))
            != Some(top)
    {
        return Err(Error::Arguments);
    }
    page.fill(0);
    let mut cursor = page.len();
    let mut pointers = [0_u64; 16];
    for (i, arg) in args.iter().enumerate().rev() {
        if arg.len() > 128 || arg.as_bytes().contains(&0) {
            return Err(Error::Arguments);
        }
        cursor = cursor.checked_sub(arg.len() + 1).ok_or(Error::Arguments)?;
        page[cursor..cursor + arg.len()].copy_from_slice(arg.as_bytes());
        pointers[i] = top - 4096 + cursor as u64;
    }
    cursor = cursor
        .checked_sub((args.len() + 3) * 8)
        .ok_or(Error::Arguments)?
        & !15;
    let rsp = top - 4096 + cursor as u64;
    for value in core::iter::once(args.len() as u64)
        .chain(pointers[..args.len()].iter().copied())
        .chain([0, 0])
    {
        page[cursor..cursor + 8].copy_from_slice(&value.to_le_bytes());
        cursor += 8;
    }
    Ok(rsp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_aligned_user_pointers_null_argv_and_empty_environment() {
        let mut bytes = [0xff; 4096];
        let top = anova_abi::STACK_TOP;
        let rsp = build(&["hello", "two words", ""], &mut bytes, top).unwrap();
        assert_eq!(rsp & 15, 0);
        let offset = (rsp - (top - 4096)) as usize;
        let read = |i: usize| {
            u64::from_le_bytes(
                bytes[offset + i * 8..offset + i * 8 + 8]
                    .try_into()
                    .unwrap(),
            )
        };
        assert_eq!(read(0), 3);
        assert_eq!(read(4), 0);
        assert_eq!(read(5), 0);
        let arg = (read(2) - (top - 4096)) as usize;
        assert_eq!(&bytes[arg..arg + 10], b"two words\0");
        assert!(build(&[], &mut bytes, top).is_err());
        assert!(build(&["bad\0arg"], &mut bytes, top).is_err());
        assert!(build(&["hello"], &mut bytes, 0).is_err());
    }
}
