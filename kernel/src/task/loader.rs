// SPDX-License-Identifier: Apache-2.0
//! Load native or Linux executable/interpreter pairs, private pages and checked startup stacks.
//! Permission decisions precede activation; physical aliases initialize RX data.
use crate::mm::user::UserSpace;
use anova_abi::{PERSONALITY, STACK_BYTES, STACK_TOP};
use anova_memory::paging::Permissions;

#[derive(Debug)]
pub enum LoadError {
    Elf(anova_elf::Error),
    Memory(anova_memory::Error),
    Personality,
    Entropy(i64),
    Interpreter(i64),
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
            Self::Entropy(error) => write!(f, "Linux startup entropy unavailable: {error}"),
            Self::Interpreter(error) => write!(f, "Linux interpreter unavailable: errno={error}"),
        }
    }
}

pub struct Loaded {
    pub linux: bool,
    pub brk: u64,
    pub clear_tid: u64,
    pub sigmask: u64,
    pub ignored: u64,
    pub pending: u64,
    pub actions:
        anova_memory::shared::Shared<core::cell::RefCell<[super::linux::signals::Action; 64]>>,
    pub space: UserSpace,
    pub entry: u64,
    pub stack: u64,
}

#[cfg(feature = "qemu-test")]
pub fn load(bytes: &[u8], personality: &str, args: &[&str]) -> Result<Loaded, LoadError> {
    load_env(bytes, personality, args, &[])
}
#[cfg(feature = "qemu-test")]
pub fn load_env(
    bytes: &[u8],
    personality: &str,
    args: &[&str],
    env: &[&str],
) -> Result<Loaded, LoadError> {
    load_interpreter(bytes, personality, args, env, None)
}
pub fn load_interpreter(
    bytes: &[u8],
    personality: &str,
    args: &[&str],
    env: &[&str],
    interpreter: Option<&[u8]>,
) -> Result<Loaded, LoadError> {
    let linux = personality == anova_abi::linux::PERSONALITY;
    if personality != PERSONALITY && !linux {
        return Err(LoadError::Personality);
    }
    let image = if linux {
        anova_elf::parse_linux(bytes)?
    } else {
        anova_elf::parse(bytes)?
    };
    let dynamic = if linux {
        anova_elf::interpreter(bytes)?
    } else {
        None
    };
    let interpreter = match (dynamic, interpreter) {
        (Some(_), Some(data)) => Some((data, anova_elf::parse_linux_at(data, 0x2000_0000)?)),
        (Some(_), None) => return Err(LoadError::Personality),
        (None, _) => None,
    };
    let entry = interpreter
        .as_ref()
        .map_or(image.entry, |(_, elf)| elf.entry);
    let mut stack_page = [0; 4096];
    let stack = if linux {
        let mut random = [0; 16];
        super::linux::random(&mut random).map_err(LoadError::Entropy)?;
        anova_elf::stack::build_linux_interpreter(
            args,
            env,
            &mut stack_page,
            STACK_TOP,
            &image,
            random,
            if interpreter.is_some() {
                0x2000_0000
            } else {
                0
            },
        )?
    } else {
        anova_elf::stack::build(args, &mut stack_page, STACK_TOP)?
    };
    let mut space = UserSpace::new()?;
    map_image(&mut space, bytes, &image)?;
    if let Some((data, elf)) = interpreter {
        map_image(&mut space, data, &elf)?;
    }
    for page in ((STACK_TOP - STACK_BYTES)..STACK_TOP).step_by(4096) {
        space.map(page, Permissions::DATA)?;
    }
    space.initialize(STACK_TOP - 4096, &stack_page)?;
    Ok(Loaded {
        linux,
        brk: 0x4000_0000,
        clear_tid: 0,
        sigmask: 0,
        ignored: 0,
        pending: 0,
        actions: anova_memory::shared::Shared::try_new(core::cell::RefCell::new(
            [super::linux::signals::Action::default(); 64],
        ))?,
        space,
        entry,
        stack,
    })
}

fn map_image(
    space: &mut UserSpace,
    bytes: &[u8],
    image: &anova_elf::Image,
) -> Result<(), LoadError> {
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
    Ok(())
}
