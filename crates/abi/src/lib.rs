// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Native v1 constants, checked addresses and layouts; Linux translations live in a distinct module.
//! Top-level syscall constants are native AnovaOS interfaces, never Linux syscall numbers.

pub mod files;
pub mod linux;
pub mod process;
pub mod pty;

pub const VERSION: u32 = 1;
/// SDK releases are independent from the frozen native syscall wire version.
pub const SDK_VERSION: &str = "1.0.0";
pub const SDK_MAJOR: u32 = 1;
pub const PERSONALITY: &str = "anova.native.v1";
pub const SYS_WRITE: u64 = 0;
pub const SYS_EXIT: u64 = 1;
pub const SYS_INFO: u64 = 2;
pub const SYS_YIELD: u64 = 3;
pub const SYS_SLEEP: u64 = 4;
pub const SYS_SPAWN: u64 = 5;
pub const SYS_EXEC: u64 = 6;
pub const SYS_WAIT: u64 = 7;
pub const SYS_FORK: u64 = 8;
pub const SYS_CLOSE: u64 = 9;
pub const SYS_READ: u64 = 10;
pub const SYS_OPEN: u64 = 11;
pub const SYS_SEEK: u64 = 12;
pub const SYS_DUP2: u64 = 13;
pub const SYS_PIPE: u64 = 14;
pub const SYS_READDIR: u64 = 15;
pub const SYS_MKDIR: u64 = 16;
pub const SYS_CHDIR: u64 = 17;
pub const SYS_GETCWD: u64 = 18;
pub const SYS_UNLINK: u64 = 19;
pub const SYS_SYNC: u64 = 20;
pub const SYS_FSYNC: u64 = 21;
pub const SYS_SHUTDOWN: u64 = 22;
pub const SYS_SET_EXECUTABLE: u64 = 23;
pub const OPEN_READ: u64 = 1;
pub const OPEN_WRITE: u64 = 2;
pub const OPEN_CREATE: u64 = 4;
pub const OPEN_TRUNCATE: u64 = 8;
pub const OPEN_APPEND: u64 = 16;
pub const OPEN_DIRECTORY: u64 = 32;
pub const OPEN_EXCLUSIVE: u64 = 64;
pub const OPEN_FLAGS: u64 = 127;
pub const FILE_LIMIT: i64 = -10;
pub const READ_ONLY: i64 = -11;
pub const NOT_DIRECTORY: i64 = -12;
pub const IS_DIRECTORY: i64 = -13;
pub const ALREADY_EXISTS: i64 = -14;
pub const WOULD_BLOCK: i64 = -15;
pub const BROKEN_PIPE: i64 = -16;
pub const NOT_SEEKABLE: i64 = -17;
pub const NO_SPACE: i64 = -18;
pub const LINK_LOOP: i64 = -19;
pub const PERMISSION_DENIED: i64 = -20;
pub const IO_ERROR: i64 = -21;
pub const INVALID_FILESYSTEM: i64 = -22;
pub const DEVICE_TIMEOUT: i64 = -23;
pub const BUSY: i64 = -24;
pub const NOT_EMPTY: i64 = -25;

/// Fixed little-endian directory record, with all 80 bytes initialized.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub inode: u64,
    pub kind: u32,
    pub name_len: u32,
    pub name: [u8; 64],
}
impl Default for DirEntry {
    fn default() -> Self {
        Self {
            inode: 0,
            kind: 0,
            name_len: 0,
            name: [0; 64],
        }
    }
}
impl DirEntry {
    pub fn bytes(self) -> [u8; 80] {
        let mut bytes = [0; 80];
        bytes[..8].copy_from_slice(&self.inode.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.kind.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.name_len.to_le_bytes());
        bytes[16..].copy_from_slice(&self.name);
        bytes
    }
}
const _: () = assert!(size_of::<DirEntry>() == 80 && align_of::<DirEntry>() == 8);
pub const BAD_ADDRESS: i64 = -1;
pub const UNSUPPORTED: i64 = -2;
pub const INVALID_ARGUMENT: i64 = -3;
pub const TOO_LONG: i64 = -4;
pub const NO_CHILD: i64 = -5;
pub const NO_MEMORY: i64 = -6;
pub const NOT_FOUND: i64 = -7;
pub const PROCESS_LIMIT: i64 = -8;
pub const BAD_DESCRIPTOR: i64 = -9;
pub const MAX_NAME: usize = 128;
/// Native startup limits; the combined strings/pointer table must also fit the startup page.
pub const MAX_ARGS: usize = 64;
pub const MAX_ARGUMENT_BYTES: usize = 1023;

/// A negative native v1 error, preserving unknown future values without errno conversion.
///
/// Native errors are process return values, never thread-local Linux `errno` values.
/// SDK additions may introduce names, but existing values and meanings never change in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Error(i64);

impl Error {
    pub const BAD_ADDRESS: Self = Self(BAD_ADDRESS);
    pub const UNSUPPORTED: Self = Self(UNSUPPORTED);
    pub const INVALID_ARGUMENT: Self = Self(INVALID_ARGUMENT);
    pub const NOT_FOUND: Self = Self(NOT_FOUND);
    pub const BAD_DESCRIPTOR: Self = Self(BAD_DESCRIPTOR);

    pub const fn from_raw(value: i64) -> Option<Self> {
        if value < 0 { Some(Self(value)) } else { None }
    }

    pub const fn raw(self) -> i64 {
        self.0
    }

    pub const fn name(self) -> &'static str {
        match self.0 {
            BAD_ADDRESS => "bad_address",
            UNSUPPORTED => "unsupported",
            INVALID_ARGUMENT => "invalid_argument",
            TOO_LONG => "too_long",
            NO_CHILD => "no_child",
            NO_MEMORY => "no_memory",
            NOT_FOUND => "not_found",
            PROCESS_LIMIT => "process_limit",
            BAD_DESCRIPTOR => "bad_descriptor",
            FILE_LIMIT => "file_limit",
            READ_ONLY => "read_only",
            NOT_DIRECTORY => "not_directory",
            IS_DIRECTORY => "is_directory",
            ALREADY_EXISTS => "already_exists",
            WOULD_BLOCK => "would_block",
            BROKEN_PIPE => "broken_pipe",
            NOT_SEEKABLE => "not_seekable",
            NO_SPACE => "no_space",
            LINK_LOOP => "link_loop",
            PERMISSION_DENIED => "permission_denied",
            IO_ERROR => "io_error",
            INVALID_FILESYSTEM => "invalid_filesystem",
            DEVICE_TIMEOUT => "device_timeout",
            BUSY => "busy",
            NOT_EMPTY => "not_empty",
            _ => "unknown_native_error",
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{} ({})", self.name(), self.raw())
    }
}

impl core::error::Error for Error {}

/// Decode a native syscall result while retaining every negative error verbatim.
pub const fn decode(value: i64) -> Result<u64, Error> {
    match Error::from_raw(value) {
        Some(error) => Err(error),
        None => Ok(value as u64),
    }
}

/// Explicit little-endian wire values; fields contain user addresses, never references.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ExecRequest {
    pub name: u64,
    pub name_len: u64,
    pub argument: u64,
    pub argument_len: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct ExitStatus {
    /// 0=exit, 1=CPU fault (vector), 2=unsafe return rejected, 3=signal termination (number).
    pub reason: u32,
    pub code: u32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct WaitStatus {
    pub pid: u64,
    pub status: ExitStatus,
}

impl WaitStatus {
    pub fn bytes(self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.pid.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.status.reason.to_le_bytes());
        bytes[12..].copy_from_slice(&self.status.code.to_le_bytes());
        bytes
    }
}

const _: () = assert!(size_of::<ExecRequest>() == 32 && size_of::<WaitStatus>() == 16);
pub const MAX_WRITE: usize = 4096;
pub const USER_MIN: u64 = 0x1_0000;
pub const USER_LIMIT: u64 = 1 << 47;
pub const STACK_TOP: u64 = USER_LIMIT - 0x1_0000;
pub const STACK_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserAddr(pub u64);

impl UserAddr {
    /// Empty copies do not dereference an address, including null.
    pub fn checked_end(self, length: usize) -> Option<u64> {
        if length == 0 {
            return Some(self.0);
        }
        let end = self.0.checked_add(u64::try_from(length).ok()?)?;
        (self.0 >= USER_MIN && end <= USER_LIMIT).then_some(end)
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct SystemInfo {
    pub abi_version: u32,
    pub size: u32,
    pub page_size: u64,
    pub ticks: u64,
    pub process_id: u64,
}

impl SystemInfo {
    /// Serialize fields explicitly, never expose Rust padding or kernel pointers.
    pub fn bytes(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[0..4].copy_from_slice(&self.abi_version.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.size.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.page_size.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.ticks.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.process_id.to_le_bytes());
        bytes
    }
}

const _: () = assert!(size_of::<SystemInfo>() == 32 && align_of::<SystemInfo>() == 8);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_v1_numbers_and_layouts_are_frozen() {
        assert_eq!(VERSION, 1);
        assert_eq!(PERSONALITY, "anova.native.v1");
        assert_eq!(
            [
                SYS_WRITE,
                SYS_EXIT,
                SYS_INFO,
                SYS_YIELD,
                SYS_SLEEP,
                SYS_SPAWN,
                SYS_EXEC,
                SYS_WAIT,
                SYS_FORK,
                SYS_CLOSE,
                SYS_READ,
                SYS_OPEN,
                SYS_SEEK,
                SYS_DUP2,
                SYS_PIPE,
                SYS_READDIR,
                SYS_MKDIR,
                SYS_CHDIR,
                SYS_GETCWD,
                SYS_UNLINK,
                SYS_SYNC,
                SYS_FSYNC,
                SYS_SHUTDOWN,
                SYS_SET_EXECUTABLE
            ],
            core::array::from_fn::<_, 24, _>(|index| index as u64)
        );
        assert_eq!(size_of::<ExecRequest>(), 32);
        assert_eq!(align_of::<ExecRequest>(), 8);
        assert_eq!(core::mem::offset_of!(ExecRequest, argument), 16);
        assert_eq!(size_of::<WaitStatus>(), 16);
        assert_eq!(core::mem::offset_of!(WaitStatus, status), 8);
        assert_eq!(size_of::<DirEntry>(), 80);
        assert_eq!(align_of::<DirEntry>(), 8);
        assert_eq!(core::mem::offset_of!(DirEntry, name), 16);
        assert_eq!(core::mem::offset_of!(SystemInfo, process_id), 24);
        assert_eq!(OPEN_FLAGS, 127);
        assert_eq!(MAX_ARGS, 64);
        assert_eq!(MAX_ARGUMENT_BYTES, 1023);
    }

    #[test]
    fn native_errors_keep_numeric_identity_and_unknown_values() {
        let errors = [
            BAD_ADDRESS,
            UNSUPPORTED,
            INVALID_ARGUMENT,
            TOO_LONG,
            NO_CHILD,
            NO_MEMORY,
            NOT_FOUND,
            PROCESS_LIMIT,
            BAD_DESCRIPTOR,
            FILE_LIMIT,
            READ_ONLY,
            NOT_DIRECTORY,
            IS_DIRECTORY,
            ALREADY_EXISTS,
            WOULD_BLOCK,
            BROKEN_PIPE,
            NOT_SEEKABLE,
            NO_SPACE,
            LINK_LOOP,
            PERMISSION_DENIED,
            IO_ERROR,
            INVALID_FILESYSTEM,
            DEVICE_TIMEOUT,
            BUSY,
            NOT_EMPTY,
        ];
        for (index, raw) in errors.into_iter().enumerate() {
            assert_eq!(raw, -(index as i64 + 1));
            let error = Error::from_raw(raw).unwrap();
            assert_eq!(error.raw(), raw);
            assert_ne!(error.name(), "unknown_native_error");
            assert_eq!(decode(raw), Err(error));
        }
        for raw in [i64::MIN, -26, -4096] {
            let error = Error::from_raw(raw).unwrap();
            assert_eq!(error.raw(), raw);
            assert_eq!(error.name(), "unknown_native_error");
            assert_eq!(decode(raw), Err(error));
        }
        assert_eq!(Error::from_raw(0), None);
        assert_eq!(Error::from_raw(1), None);
        assert_eq!(decode(i64::MAX), Ok(i64::MAX as u64));
    }
    #[test]
    fn user_range_rejects_overflow_kernel_and_noncanonical_addresses() {
        for address in [0, USER_MIN - 1, USER_LIMIT, 0xffff_8000_0000_0000, u64::MAX] {
            assert!(UserAddr(address).checked_end(1).is_none());
        }
        assert_eq!(UserAddr(USER_LIMIT - 1).checked_end(1), Some(USER_LIMIT));
        assert!(UserAddr(USER_LIMIT - 1).checked_end(2).is_none());
        assert_eq!(UserAddr(0).checked_end(0), Some(0));
    }
    #[test]
    fn info_wire_layout_has_no_unspecified_bytes() {
        let bytes = SystemInfo {
            abi_version: VERSION,
            size: 32,
            page_size: 4096,
            ticks: 8,
            process_id: 1,
        }
        .bytes();
        assert_eq!(u64::from_le_bytes(bytes[24..32].try_into().unwrap()), 1);
        assert_eq!(&bytes[..8], &[1, 0, 0, 0, 32, 0, 0, 0]);
    }
}
