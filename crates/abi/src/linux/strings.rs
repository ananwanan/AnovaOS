// SPDX-License-Identifier: Apache-2.0
//! Decode bounded Linux C strings and pointer vectors through a checked copy callback.
//! Copies stop at page boundaries so a terminator before an unmapped page remains valid.
//! Exec vectors enforce independent data and entry quotas. The string bound is shared
//! with the kernel's Linux ELF startup stack encoder.
use super::{E2BIG, EFAULT, EINVAL, ENAMETOOLONG};

pub const STRING_BYTES: usize = 129;
pub const VECTOR_LIMIT: usize = 512;
pub const EXEC_STRING_BYTES: usize = 12 * 1024 - 1;
pub const EXEC_VECTOR_BYTES: usize = 16 * 1024;
const PAGE_BYTES: u64 = 4096;

/// The caller pins the address space and recovers access faults inside `copy`.
pub fn read_c_string<const N: usize>(
    address: u64,
    bytes: &mut [u8; N],
    copy: &mut impl FnMut(u64, &mut [u8]) -> Result<(), i64>,
) -> Result<usize, i64> {
    let mut done = 0;
    while done < bytes.len() {
        let at = address.checked_add(done as u64).ok_or(EFAULT)?;
        let count = (bytes.len() - done).min((PAGE_BYTES - at % PAGE_BYTES) as usize);
        let end = done + count;
        copy(at, &mut bytes[done..end])?;
        if let Some(offset) = bytes[done..end].iter().position(|byte| *byte == 0) {
            let length = done + offset;
            core::str::from_utf8(&bytes[..length]).map_err(|_| EINVAL)?;
            return Ok(length);
        }
        done = end;
    }
    Err(ENAMETOOLONG)
}

/// Own validated argv/envp bytes while the original user mappings are inactive or replaced.
pub struct StringVector {
    bytes: [u8; EXEC_VECTOR_BYTES],
    offsets: [usize; VECTOR_LIMIT],
    lengths: [usize; VECTOR_LIMIT],
    count: usize,
}
impl StringVector {
    pub fn empty() -> Self {
        Self {
            bytes: [0; EXEC_VECTOR_BYTES],
            offsets: [0; VECTOR_LIMIT],
            lengths: [0; VECTOR_LIMIT],
            count: 0,
        }
    }
    pub fn read(
        &mut self,
        address: u64,
        copy: &mut impl FnMut(u64, &mut [u8]) -> Result<(), i64>,
    ) -> Result<(), i64> {
        self.count = 0;
        self.lengths.fill(0);
        let mut used = 0_usize;
        let mut string = [0; EXEC_STRING_BYTES + 1];
        for index in 0..=VECTOR_LIMIT {
            let at = address.checked_add(index as u64 * 8).ok_or(EFAULT)?;
            let mut word = [0; 8];
            copy(at, &mut word)?;
            let pointer = u64::from_le_bytes(word);
            if pointer == 0 {
                return Ok(());
            }
            if index == VECTOR_LIMIT {
                return Err(E2BIG);
            }
            let length = read_c_string(pointer, &mut string, copy)
                .map_err(|error| if error == ENAMETOOLONG { E2BIG } else { error })?;
            let end = used
                .checked_add(length)
                .filter(|end| *end <= self.bytes.len())
                .ok_or(E2BIG)?;
            self.bytes[used..end].copy_from_slice(&string[..length]);
            self.offsets[index] = used;
            self.lengths[index] = length;
            used = end;
            self.count += 1;
        }
        unreachable!("Bounded vector must terminate or exceed capacity");
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn strings(&self) -> [&str; VECTOR_LIMIT] {
        core::array::from_fn(|index| {
            // Only read() sets lengths, after validating both UTF-8 and the bound.
            core::str::from_utf8(
                &self.bytes[self.offsets[index]..self.offsets[index] + self.lengths[index]],
            )
            .expect("Previously validated Linux string")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::vec;

    #[test]
    fn copies_maximum_string_once_without_crossing_pages() {
        let input = [b'x'; STRING_BYTES];
        let mut output = [0; STRING_BYTES];
        let mut calls = 0;
        assert_eq!(
            read_c_string(0x10000, &mut output, &mut |_, bytes| {
                calls += 1;
                bytes.copy_from_slice(&input);
                bytes[STRING_BYTES - 1] = 0;
                Ok(())
            }),
            Ok(128)
        );
        assert_eq!(calls, 1);
        assert_eq!(&output[..128], &input[..128]);
    }

    #[test]
    fn accepts_page_end_terminator_and_faults_only_when_another_page_is_needed() {
        let mut output = [0; STRING_BYTES];
        let mut calls = 0;
        let mut copy = |address, bytes: &mut [u8]| {
            calls += 1;
            if address != 0x10ffc || bytes.len() != 4 {
                return Err(EFAULT);
            }
            bytes.copy_from_slice(b"abc\0");
            Ok(())
        };
        assert_eq!(read_c_string(0x10ffc, &mut output, &mut copy), Ok(3));
        assert_eq!(calls, 1);
        assert_eq!(
            read_c_string(0x10ffc, &mut output, &mut |address, bytes| {
                if address != 0x10ffc {
                    return Err(EFAULT);
                }
                bytes.fill(b'x');
                Ok(())
            }),
            Err(EFAULT)
        );
        let mut calls = 0;
        assert_eq!(
            read_c_string(0x10ffc, &mut output, &mut |address, bytes| {
                assert!(address / PAGE_BYTES == (address + bytes.len() as u64 - 1) / PAGE_BYTES);
                calls += 1;
                bytes.fill(b'x');
                if calls == 2 {
                    bytes[2] = 0;
                }
                Ok(())
            }),
            Ok(6)
        );
        assert_eq!(calls, 2);
    }

    #[test]
    fn rejects_missing_terminators_invalid_utf8_and_address_overflow() {
        let mut output = [0; STRING_BYTES];
        assert_eq!(
            read_c_string(0x10000, &mut output, &mut |_, b| {
                b.fill(b'x');
                Ok(())
            }),
            Err(ENAMETOOLONG)
        );
        assert_eq!(
            read_c_string(0x10000, &mut output, &mut |_, b| {
                b.fill(0);
                b[0] = 0xff;
                Ok(())
            }),
            Err(EINVAL)
        );
        assert_eq!(
            read_c_string(u64::MAX, &mut output, &mut |_, b| {
                b.fill(b'x');
                Ok(())
            }),
            Err(EFAULT)
        );
    }

    #[test]
    fn vectors_accept_the_full_capacity_and_reject_bad_pointers_or_excess_arguments() {
        const DATA: usize = 8192;
        const FIRST: u64 = 0x10000 + DATA as u64;
        let mut memory = vec![0; DATA + 4096];
        for index in 0..VECTOR_LIMIT {
            memory[index * 8..index * 8 + 8].copy_from_slice(&FIRST.to_le_bytes());
        }
        memory[DATA..DATA + 6].copy_from_slice(b"value\0");
        let read = |vector: &mut StringVector, address, memory: &[u8]| {
            vector.read(address, &mut |address, bytes| {
                let offset = address.checked_sub(0x10000).ok_or(EFAULT)? as usize;
                let input = memory.get(offset..offset + bytes.len()).ok_or(EFAULT)?;
                bytes.copy_from_slice(input);
                Ok(())
            })
        };
        let mut vector = StringVector::empty();
        read(&mut vector, 0x10000, &memory).unwrap();
        assert_eq!(vector.len(), VECTOR_LIMIT);
        assert!(!vector.is_empty());
        assert_eq!(vector.strings(), ["value"; VECTOR_LIMIT]);
        assert_eq!(read(&mut vector, 1, &memory), Err(EFAULT));
        memory[VECTOR_LIMIT * 8..VECTOR_LIMIT * 8 + 8].copy_from_slice(&FIRST.to_le_bytes());
        assert_eq!(read(&mut vector, 0x10000, &memory), Err(E2BIG));

        const PART: usize = 1024;
        const COUNT: usize = EXEC_VECTOR_BYTES / PART;
        memory[COUNT * 8..=VECTOR_LIMIT * 8 + 7].fill(0);
        memory[DATA..DATA + PART].fill(b'x');
        read(&mut vector, 0x10000, &memory).unwrap();
        assert_eq!(vector.len(), COUNT);
        assert_eq!(
            vector.strings()[..COUNT]
                .iter()
                .map(|s| s.len())
                .sum::<usize>(),
            EXEC_VECTOR_BYTES
        );

        const SECOND_OFFSET: usize = 0x500;
        let second = FIRST + SECOND_OFFSET as u64;
        memory[COUNT * 8..COUNT * 8 + 8].copy_from_slice(&second.to_le_bytes());
        memory[DATA + SECOND_OFFSET] = b'y';
        assert_eq!(read(&mut vector, 0x10000, &memory), Err(E2BIG));
        assert_eq!(vector.len(), COUNT);
    }

    #[test]
    fn exec_vector_accepts_a_page_crossing_string_at_the_exact_limit() {
        const BASE: u64 = 0x10000;
        const START: usize = PAGE_BYTES as usize + 3;
        let mut memory = vec![0; START + EXEC_STRING_BYTES + 1];
        memory[..8].copy_from_slice(&(BASE + START as u64).to_le_bytes());
        memory[START..START + EXEC_STRING_BYTES].fill(b'x');
        let read = |vector: &mut StringVector, memory: &[u8]| {
            vector.read(BASE, &mut |address, bytes| {
                assert_eq!(
                    address / PAGE_BYTES,
                    (address + bytes.len() as u64 - 1) / PAGE_BYTES
                );
                let offset = (address - BASE) as usize;
                bytes.copy_from_slice(&memory[offset..offset + bytes.len()]);
                Ok(())
            })
        };
        let mut vector = StringVector::empty();
        read(&mut vector, &memory).unwrap();
        assert_eq!(vector.len(), 1);
        assert_eq!(vector.strings()[0].as_bytes(), &[b'x'; EXEC_STRING_BYTES]);

        memory[START + EXEC_STRING_BYTES] = b'x';
        assert_eq!(read(&mut vector, &memory), Err(E2BIG));
        assert!(vector.is_empty());
    }
}
