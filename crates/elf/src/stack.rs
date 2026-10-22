// SPDX-License-Identifier: Apache-2.0
//! Encode native/Linux startup blocks once, with checked downward string and word placement.
use crate::Error;
use anova_abi::UserAddr;

const PAGE_BYTES: usize = 4096;
const STRING_LIMIT: usize = 64;
const STRING_BYTES: usize = 1023;

struct StackPage<'a> {
    page: &'a mut [u8],
    base: u64,
    cursor: usize,
}
impl<'a> StackPage<'a> {
    fn new(page: &'a mut [u8], top: u64) -> Result<Self, Error> {
        let base = top.checked_sub(page.len() as u64).ok_or(Error::Arguments)?;
        if top & (PAGE_BYTES as u64 - 1) != 0
            || UserAddr(base).checked_end(page.len()) != Some(top)
            || page.is_empty()
            || page.len() > 16384
            || !page.len().is_multiple_of(PAGE_BYTES)
        {
            return Err(Error::Arguments);
        }
        page.fill(0);
        let cursor = page.len();
        Ok(Self { page, base, cursor })
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<u64, Error> {
        let start = self
            .cursor
            .checked_sub(bytes.len())
            .ok_or(Error::Arguments)?;
        self.page[start..self.cursor].copy_from_slice(bytes);
        self.cursor = start;
        Ok(self.base + start as u64)
    }
    fn strings(&mut self, strings: &[&str]) -> Result<[u64; STRING_LIMIT], Error> {
        if strings.len() > STRING_LIMIT {
            return Err(Error::Arguments);
        }
        let mut pointers = [0; STRING_LIMIT];
        for (index, string) in strings.iter().enumerate().rev() {
            if string.len() > STRING_BYTES || string.as_bytes().contains(&0) {
                return Err(Error::Arguments);
            }
            self.bytes(&[0])?;
            pointers[index] = self.bytes(string.as_bytes())?;
        }
        Ok(pointers)
    }
    fn finish(self, argv: &[u64], envp: &[u64], auxv: &[(u64, u64)]) -> Result<u64, Error> {
        let words = argv.len() + envp.len() + 3 + auxv.len() * 2;
        let start = self.cursor.checked_sub(words * 8).ok_or(Error::Arguments)? & !15;
        let values = core::iter::once(argv.len() as u64)
            .chain(argv.iter().copied())
            .chain([0])
            .chain(envp.iter().copied())
            .chain([0])
            .chain(auxv.iter().flat_map(|&(kind, value)| [kind, value]));
        for (slot, value) in self.page[start..start + words * 8]
            .chunks_exact_mut(8)
            .zip(values)
        {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        Ok(self.base + start as u64)
    }
}

pub fn build(args: &[&str], page: &mut [u8], top: u64) -> Result<u64, Error> {
    if args.is_empty() {
        return Err(Error::Arguments);
    }
    let mut stack = StackPage::new(page, top)?;
    let argv = stack.strings(args)?;
    stack.finish(&argv[..args.len()], &[], &[])
}

/// Build the Linux AMD64 entry contract, including an AT_NULL-terminated aux vector.
/// Random bytes must come from the kernel entropy source, never a fixed fixture in production.
pub fn build_linux(
    args: &[&str],
    page: &mut [u8],
    top: u64,
    image: &crate::Image,
    random: [u8; 16],
) -> Result<u64, Error> {
    build_linux_env(args, &[], page, top, image, random)
}

pub fn build_linux_env(
    args: &[&str],
    env: &[&str],
    page: &mut [u8],
    top: u64,
    image: &crate::Image,
    random: [u8; 16],
) -> Result<u64, Error> {
    build_linux_interpreter(args, env, page, top, image, random, 0)
}

pub fn build_linux_interpreter(
    args: &[&str],
    env: &[&str],
    page: &mut [u8],
    top: u64,
    image: &crate::Image,
    random: [u8; 16],
    interpreter_base: u64,
) -> Result<u64, Error> {
    if args.is_empty() {
        return Err(Error::Arguments);
    }
    let mut stack = StackPage::new(page, top)?;
    let argv = stack.strings(args)?;
    let envp = stack.strings(env)?;
    let random = stack.bytes(&random)?;
    let auxv = [
        (3, image.phdr),
        (4, 56),
        (5, image.phnum),
        (6, PAGE_BYTES as u64),
        (7, interpreter_base),
        (8, 0),
        (9, image.entry),
        (11, 0),
        (12, 0),
        (13, 0),
        (14, 0),
        (23, 0),
        (25, random),
        (0, 0),
    ];
    stack.finish(&argv[..args.len()], &envp[..env.len()], &auxv)
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
        let fitting = "x".repeat(106);
        assert!(
            build_linux_env(
                &[fitting.as_str(); 16],
                &[fitting.as_str(); 16],
                &mut page,
                top,
                &image,
                [0x5a; 16]
            )
            .is_ok()
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
