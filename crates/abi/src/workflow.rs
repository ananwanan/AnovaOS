// SPDX-License-Identifier: Apache-2.0
//! Admit fixed desktop compiler recipes, owner cancellation and checked publication.
//! This separate native interface keeps the legacy controlled GCC job wire format intact.

pub const SYSCALL: u64 = 0x106;
pub const VERSION: u32 = 1;
pub const START: u32 = 0;
pub const READ: u32 = 1;
pub const STATUS: u32 = 2;
pub const CLOSE: u32 = 3;
pub const CANCEL: u32 = 4;
pub const PUBLISH: u32 = 5;
pub const COPY_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u64)]
pub enum Recipe {
    #[default]
    C = 1,
    Cpp = 2,
    Rust = 3,
}

impl Recipe {
    pub const fn decode(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::C),
            2 => Some(Self::Cpp),
            3 => Some(Self::Rust),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Rust => "Rust",
        }
    }

    pub const fn args(self) -> &'static [&'static str] {
        match self {
            Self::C => &[
                "/bin/gcc",
                "-fdiagnostics-color=never",
                super::build::SOURCE,
                "-o",
                "/tmp/desktop-c.out",
            ],
            Self::Cpp => &[
                "/bin/g++",
                "-std=c++11",
                "-fdiagnostics-color=never",
                "/persist/build.cpp",
                "/persist/helper.cpp",
                "-o",
                "/tmp/desktop-cpp.out",
            ],
            Self::Rust => &[
                "/bin/rustc",
                "--edition=2024",
                "--color=never",
                "/persist/build.rs",
                "-o",
                "/tmp/desktop-rust.out",
            ],
        }
    }

    pub const fn staging(self) -> &'static str {
        match self {
            Self::C => super::build::STAGING,
            Self::Cpp => "/persist/cpp-app.next",
            Self::Rust => "/persist/rust-app.next",
        }
    }

    pub const fn temporary(self) -> &'static str {
        let args = self.args();
        args[args.len() - 1]
    }

    pub const fn output(self) -> &'static str {
        match self {
            Self::C => super::build::OUTPUT,
            Self::Cpp => "/persist/cpp-app",
            Self::Rust => "/persist/rust-app",
        }
    }
}

/// RDI is version:operation. RSI is a recipe for START or the exact owned job ID.
/// RDX names the existing 32-byte build transfer record for all operations.
/// CANCEL and PUBLISH require an all-zero record.
pub const fn command(operation: u32) -> u64 {
    (VERSION as u64) << 32 | operation as u64
}

pub const fn decode(value: u64) -> Option<u32> {
    if value >> 32 == VERSION as u64 && value as u32 <= PUBLISH {
        Some(value as u32)
    } else {
        None
    }
}

pub fn valid(request: super::build::Request, operation: u32) -> bool {
    match operation {
        CANCEL | PUBLISH => request.valid(super::build::CLOSE),
        _ => request.valid(operation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{self, Request};

    #[test]
    fn recipes_select_only_declared_paths_and_transfer_bounds() {
        for recipe in [Recipe::C, Recipe::Cpp, Recipe::Rust] {
            assert_eq!(Recipe::decode(recipe as u64), Some(recipe));
            assert_eq!(
                recipe.staging().strip_suffix(".next"),
                Some(recipe.output())
            );
            assert!(recipe.temporary().starts_with("/tmp/"));
        }
        assert_eq!(Recipe::decode(0), None);
        assert_eq!(Recipe::decode(4), None);
        assert!(valid(Request::default(), CANCEL));
        assert!(valid(Request::default(), PUBLISH));
        assert!(!valid(
            Request {
                length: 1,
                ..Request::default()
            },
            PUBLISH
        ));
        assert_eq!(decode(command(PUBLISH + 1)), None);
        assert_eq!(decode(command(PUBLISH) + (1 << 32)), None);
        assert_eq!(build::decode(command(CANCEL)), None);
        assert_eq!(build::decode(command(PUBLISH)), None);
    }
}
