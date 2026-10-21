// SPDX-License-Identifier: Apache-2.0
//! Encode bounded native and Linux argc/argv/envp/auxv startup blocks without host pointers.
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

/// Build the Linux AMD64 entry contract, including an AT_NULL-terminated aux vector.
/// Random bytes must come from the kernel entropy source, never a fixed fixture in production.
pub fn build_linux(
    args: &[&str],
    page: &mut [u8; 4096],
    top: u64,
    image: &crate::Image,
    random: [u8; 16],
) -> Result<u64, Error> {
    build_linux_env(args, &[], page, top, image, random)
}

pub fn build_linux_env(
    args: &[&str],
    env: &[&str],
    page: &mut [u8; 4096],
    top: u64,
    image: &crate::Image,
    random: [u8; 16],
) -> Result<u64, Error> {
    if env.len() > 16 {
        return Err(Error::Arguments);
    }
    // Reuse the checked argument encoder and preserve its strings/pointers.
    let base = build(args, page, top)?;
    let old = (base - (top - 4096)) as usize;
    let mut pointers = [0u64; 16];
    for (i, value) in pointers[..args.len()].iter_mut().enumerate() {
        *value = u64::from_le_bytes(page[old + 8 + i * 8..old + 16 + i * 8].try_into().unwrap());
    }
    let mut env_pointers = [0u64; 16];
    let mut cursor = old;
    for (i, text) in env.iter().enumerate().rev() {
        if text.len() > 128 || text.as_bytes().contains(&0) {
            return Err(Error::Arguments);
        }
        cursor = cursor.checked_sub(text.len() + 1).ok_or(Error::Arguments)?;
        page[cursor..cursor + text.len()].copy_from_slice(text.as_bytes());
        page[cursor + text.len()] = 0;
        env_pointers[i] = top - 4096 + cursor as u64;
    }
    let random_offset = cursor.checked_sub(16).ok_or(Error::Arguments)?;
    page[random_offset..random_offset + 16].copy_from_slice(&random);
    let aux = [
        (3, image.phdr),
        (4, 56),
        (5, image.phnum),
        (6, 4096),
        (7, 0),
        (8, 0),
        (9, image.entry),
        (11, 0),
        (12, 0),
        (13, 0),
        (14, 0),
        (23, 0),
        (25, top - 4096 + random_offset as u64),
        (0, 0),
    ];
    let start = random_offset
        .checked_sub((args.len() + env.len() + 3 + aux.len() * 2) * 8)
        .ok_or(Error::Arguments)?
        & !15;
    let mut cursor = start;
    for value in core::iter::once(args.len() as u64)
        .chain(pointers[..args.len()].iter().copied())
        .chain([0])
        .chain(env_pointers[..env.len()].iter().copied())
        .chain([0])
        .chain(aux.into_iter().flat_map(|(a, b)| [a, b]))
    {
        page[cursor..cursor + 8].copy_from_slice(&value.to_le_bytes());
        cursor += 8;
    }
    Ok(top - 4096 + start as u64)
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
    #[test]
    fn linux_stack_encodes_environment_auxv_random_and_bounded_overflow() {
        let image = crate::Image {
            entry: 0x401000,
            phdr: 0x400040,
            phnum: 7,
            segments: [crate::Segment::default(); crate::MAX_SEGMENTS],
            count: 0,
        };
        let mut page = [0; 4096];
        let top = anova_abi::STACK_TOP;
        let rsp = build_linux_env(
            &["app", "arg"],
            &["M8=environment"],
            &mut page,
            top,
            &image,
            [0x5a; 16],
        )
        .unwrap();
        assert_eq!(rsp & 15, 0);
        let offset = (rsp - (top - 4096)) as usize;
        let word = |i: usize| {
            u64::from_le_bytes(page[offset + i * 8..offset + i * 8 + 8].try_into().unwrap())
        };
        assert_eq!(word(0), 2);
        assert_eq!(word(3), 0);
        let env = (word(4) - (top - 4096)) as usize;
        assert_eq!(&page[env..env + 15], b"M8=environment\0");
        assert_eq!(word(5), 0);
        let mut at = 6;
        let mut found = false;
        while word(at) != 0 {
            if word(at) == 25 {
                let random = (word(at + 1) - (top - 4096)) as usize;
                assert_eq!(&page[random..random + 16], &[0x5a; 16]);
                found = true;
            }
            at += 2;
        }
        assert!(found);
        assert_eq!(word(at + 1), 0);
        assert!(
            build_linux_env(&["app"], &["bad\0value"], &mut page, top, &image, [0; 16]).is_err()
        );
        let long = "x".repeat(128);
        assert!(
            build_linux_env(
                &[long.as_str(); 16],
                &[long.as_str(); 16],
                &mut page,
                top,
                &image,
                [0; 16]
            )
            .is_err()
        );
    }
}
