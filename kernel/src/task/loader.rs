// SPDX-License-Identifier: Apache-2.0
//! Load a validated native ET_EXEC into zeroed private pages and build argc/argv.
//! Permission decisions precede activation; physical aliases initialize RX data.
use crate::mm::user::UserSpace;
use anova_abi::{PERSONALITY, STACK_BYTES, STACK_TOP};
use anova_memory::paging::Permissions;

#[derive(Debug)]
pub enum LoadError {
    Elf(anova_elf::Error),
    Memory(anova_memory::Error),
    Personality,
}
impl From<anova_elf::Error> for LoadError {
    fn from(e: anova_elf::Error) -> Self {
        Self::Elf(e)
    }
}
impl From<anova_memory::Error> for LoadError {
    fn from(e: anova_memory::Error) -> Self {
        Self::Memory(e)
    }
}
impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Elf(e) => write!(f, "ELF: {e:?}"),
            Self::Memory(e) => write!(f, "Memory: {e:?}"),
            Self::Personality => write!(f, "Unsupported ABI personality"),
        }
    }
}

pub struct Loaded {
    pub space: UserSpace,
    pub entry: u64,
    pub stack: u64,
}

pub fn load(bytes: &[u8], personality: &str, args: &[&str]) -> Result<Loaded, LoadError> {
    if personality != PERSONALITY {
        return Err(LoadError::Personality);
    }
    let image = anova_elf::parse(bytes)?;
    let mut stack_page = [0; 4096];
    let stack = anova_elf::stack::build(args, &mut stack_page, STACK_TOP)?;
    let mut space = UserSpace::new()?;
    for segment in image.segments() {
        let permissions = Permissions {
            writable: segment.writable,
            executable: segment.executable,
            ..Permissions::READ_ONLY
        };
        for page in (segment.page_start()..segment.page_end()).step_by(4096) {
            space.map(page, permissions)?;
        }
        space.initialize(
            segment.address,
            &bytes[segment.file_offset..segment.file_offset + segment.file_size],
        )?;
    }
    for page in ((STACK_TOP - STACK_BYTES)..STACK_TOP).step_by(4096) {
        space.map(page, Permissions::DATA)?;
    }
    space.initialize(STACK_TOP - 4096, &stack_page)?;
    Ok(Loaded {
        space,
        entry: image.entry,
        stack,
    })
}
