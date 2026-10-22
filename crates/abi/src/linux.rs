// SPDX-License-Identifier: Apache-2.0
//! Define Linux error/flag translations and bounded string decoding at the ABI boundary.
//! No Linux syscall numbers or wire layouts belong in generic VFS or scheduling policy.
pub mod strings;

use crate::*;
pub const PERSONALITY: &str = "linux.x86_64.v1";
pub const ENOSYS: i64 = -38;
pub const E2BIG: i64 = -7;
pub const EFAULT: i64 = -14;
pub const EINVAL: i64 = -22;
pub const ENAMETOOLONG: i64 = -36;
pub fn errno(value: i64) -> i64 {
    match value {
        BAD_ADDRESS => -14,
        UNSUPPORTED => ENOSYS,
        INVALID_ARGUMENT | INVALID_FILESYSTEM => -22,
        TOO_LONG => -36,
        NO_CHILD => -10,
        NO_MEMORY => -12,
        NOT_FOUND => -2,
        PROCESS_LIMIT => -11,
        BAD_DESCRIPTOR => -9,
        FILE_LIMIT => -24,
        READ_ONLY => -30,
        NOT_DIRECTORY => -20,
        IS_DIRECTORY => -21,
        ALREADY_EXISTS => -17,
        WOULD_BLOCK => -11,
        BROKEN_PIPE => -32,
        NOT_SEEKABLE => -29,
        NO_SPACE => -28,
        LINK_LOOP => -40,
        PERMISSION_DENIED => -13,
        IO_ERROR => -5,
        DEVICE_TIMEOUT => -110,
        n if n >= 0 => n,
        _ => -5,
    }
}
pub fn open_flags(flags: u64) -> Result<u64, i64> {
    // Access, create, exclusive, truncate, append, largefile, directory. CLOEXEC is tracked independently on the descriptor slot.
    if flags & !(3 | 0x40 | 0x80 | 0x200 | 0x400 | 0x8000 | 0x10000 | 0x80000) != 0
        || flags & 3 == 3
    {
        return Err(-22);
    }
    let mut native = match flags & 3 {
        0 => OPEN_READ,
        1 => OPEN_WRITE,
        _ => OPEN_READ | OPEN_WRITE,
    };
    for (linux, bit) in [
        (0x40, OPEN_CREATE),
        (0x80, OPEN_EXCLUSIVE),
        (0x200, OPEN_TRUNCATE),
        (0x400, OPEN_APPEND),
        (0x10000, OPEN_DIRECTORY),
    ] {
        if flags & linux != 0 {
            native |= bit;
        }
    }
    Ok(native)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translations_do_not_leak_native_error_numbers_or_unknown_flags() {
        assert_eq!(errno(BAD_ADDRESS), -14);
        assert_eq!(errno(UNSUPPORTED), -38);
        assert_eq!(errno(4096), 4096);
        assert_eq!(open_flags(0), Ok(OPEN_READ));
        assert_eq!(
            open_flags(0x241),
            Ok(OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)
        );
        for bad in [3, 0x800, u64::MAX] {
            assert_eq!(open_flags(bad), Err(-22));
        }
    }
}
