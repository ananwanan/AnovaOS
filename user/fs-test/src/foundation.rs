// SPDX-License-Identifier: Apache-2.0
//! Verify a readonly ext4 work disk through real native syscalls, streaming reads and directory records.
//! The host supplies independently checked bytes; no fixtures or disk writes are fabricated here.
use anova_rt::{self as rt, abi::*};

pub fn readonly() {
    let dir = rt::open("/persist/many", OPEN_READ | OPEN_DIRECTORY).unwrap();
    let mut seen = [0u8; 1251];
    let mut count = 0;
    while let Some(entry) = rt::readdir(dir).unwrap() {
        assert_eq!(entry.kind, 1);
        assert_eq!(entry.name_len, 6);
        assert_eq!(entry.name[0], b'f');
        let mut number = 0usize;
        for &c in &entry.name[1..6] {
            assert!(c.is_ascii_digit());
            number = number * 10 + (c - b'0') as usize;
        }
        assert!(number < 10_001);
        assert_eq!(seen[number / 8] & (1 << (number % 8)), 0);
        seen[number / 8] |= 1 << (number % 8);
        count += 1;
    }
    assert_eq!(count, 10_001);
    rt::close(dir).unwrap();
    rt::write_all(1, b"[M16:NAMESPACE] entries=10001 duplicate=false\n").unwrap();
    let fd = rt::open("/persist/large", OPEN_READ).unwrap();
    let mut bytes = [0; 4096];
    for block in 0..5120 {
        let mut done = 0;
        while done < bytes.len() {
            let n = rt::read(fd, &mut bytes[done..]).unwrap();
            assert!(n != 0);
            done += n;
        }
        assert!(bytes.iter().all(|n| *n == 1 + (block % 251) as u8));
    }
    assert_eq!(rt::read(fd, &mut bytes), Ok(0));
    rt::close(fd).unwrap();
    let fd = rt::open("/persist/sparse", OPEN_READ).unwrap();
    rt::seek(fd, 128 * 1024 * 1024, 0).unwrap();
    assert_eq!(rt::read(fd, &mut bytes), Ok(4096));
    assert!(bytes.iter().all(|n| *n == 0));
    rt::seek(fd, 199 * 1024 * 1024 + 17, 0).unwrap();
    assert_eq!(rt::read(fd, &mut bytes[..4]), Ok(4));
    assert_eq!(&bytes[..4], b"last");
    rt::close(fd).unwrap();
    assert_eq!(rt::open("/persist/large", OPEN_WRITE), Err(READ_ONLY));
    assert_eq!(
        rt::open("/persist/new", OPEN_WRITE | OPEN_CREATE),
        Err(READ_ONLY)
    );
    assert_eq!(rt::mkdir("/persist/new-dir"), Err(READ_ONLY));
    assert_eq!(rt::unlink("/persist/.anova-abi"), Err(READ_ONLY));
    assert_eq!(rt::set_executable("/persist/large"), Err(READ_ONLY));
    rt::sync().unwrap();
    rt::write_all(1, b"[M16:EXT4] large=exact sparse=zero readonly=true\n").unwrap();
}
