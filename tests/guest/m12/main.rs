// SPDX-License-Identifier: Apache-2.0
//! Exercise musl std allocation, arguments, file I/O/copy, threading and child execution.
use std::{env, fs, process::Command, thread};

fn main() {
    assert_eq!(env::args().nth(1).as_deref(), Some("guest"));
    let mut values = vec![19, 23];
    values.push(42);
    let text = format!("guest-Rust:{}", values[0] + values[1]);
    fs::write("rust-result.txt", &text).unwrap();
    assert_eq!(fs::read_to_string("rust-result.txt").unwrap(), text);
    assert_eq!(
        fs::copy("rust-result.txt", "rust-copy.txt").unwrap(),
        text.len() as u64
    );
    assert_eq!(fs::read_to_string("rust-copy.txt").unwrap(), text);
    assert_eq!(
        thread::spawn(|| String::from("thread")).join().unwrap(),
        "thread"
    );
    let child = thread::spawn(|| {
        Command::new("/linux/echo")
            .arg("m12-child")
            .output()
            .unwrap()
    })
    .join()
    .unwrap();
    assert!(child.status.success());
    assert_eq!(child.stdout, b"m12-child\n");
    println!("[M12:STD] Vec String argv files thread child");
}
