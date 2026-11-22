// SPDX-License-Identifier: Apache-2.0
//! Load native or Linux executable/interpreter pairs, private pages and checked startup stacks.
//! Permission decisions precede activation; physical aliases initialize RX data.
//! Persistent Linux exec streams validated file ranges into the new image.
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
    Read(i64),
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
            Self::Read(error) => write!(f, "Persistent executable read failed: error={error}"),
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
        (Some(_), Some(data)) => Some(data),
        (Some(_), None) => return Err(LoadError::Personality),
        (None, _) => None,
    };
    load_image(image, linux, args, env, interpreter, |space, image| {
        map_memory_image(space, image, bytes)
    })
}

pub fn load_persistent_linux(
    file: &mut anova_vfs::PersistentExecutable<'_>,
    image: anova_elf::Image,
    args: &[&str],
    env: &[&str],
    interpreter: Option<&[u8]>,
) -> Result<Loaded, LoadError> {
    load_image(image, true, args, env, interpreter, |space, image| {
        let mut buffer = [0; 16 * 1024];
        map_image(space, image, |space, segment| {
            for offset in (0..segment.file_size).step_by(buffer.len()) {
                let count = (segment.file_size - offset).min(buffer.len());
                let chunk = &mut buffer[..count];
                file.read_exact_at(segment.file_offset + offset, chunk)
                    .map_err(LoadError::Read)?;
                space.initialize(segment.address + offset as u64, chunk)?;
            }
            Ok(())
        })
    })
}

fn load_image(
    image: anova_elf::Image,
    linux: bool,
    args: &[&str],
    env: &[&str],
    interpreter: Option<&[u8]>,
    map_executable: impl FnOnce(&mut UserSpace, &anova_elf::Image) -> Result<(), LoadError>,
) -> Result<Loaded, LoadError> {
    let interpreter = if let Some(data) = interpreter {
        Some((data, anova_elf::parse_linux_at(data, 0x2000_0000)?))
    } else {
        None
    };
    let entry = interpreter
        .as_ref()
        .map_or(image.entry, |(_, elf)| elf.entry);
    let mut stack_page = [0; 32768];
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
    map_executable(&mut space, &image)?;
    if let Some((data, elf)) = interpreter {
        map_memory_image(&mut space, &elf, data)?;
    }
    let stack_bytes = if linux { 1024 * 1024 } else { STACK_BYTES };
    for page in ((STACK_TOP - stack_bytes)..STACK_TOP).step_by(4096) {
        space.map(page, Permissions::DATA)?;
    }
    space.initialize(STACK_TOP - stack_page.len() as u64, &stack_page)?;
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

fn map_memory_image(
    space: &mut UserSpace,
    image: &anova_elf::Image,
    bytes: &[u8],
) -> Result<(), LoadError> {
    map_image(space, image, |space, segment| {
        space.initialize(
            segment.address,
            &bytes[segment.file_offset..segment.file_offset + segment.file_size],
        )?;
        Ok(())
    })
}

fn map_image(
    space: &mut UserSpace,
    image: &anova_elf::Image,
    mut initialize: impl FnMut(&mut UserSpace, &anova_elf::Segment) -> Result<(), LoadError>,
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
        initialize(space, segment)?;
    }
    Ok(())
}
