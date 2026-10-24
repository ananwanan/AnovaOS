// SPDX-License-Identifier: Apache-2.0
//! Test the inherited Rust seed's standard library; this does not rebuild a compiler.
use std::{fs, thread};

fn main() {
    let values = vec![19, 23];
    let value = thread::spawn(move || values.into_iter().sum::<i32>())
        .join()
        .unwrap();
    assert_eq!(value, 42);
    let text = format!("seed-rust:{value}\n");
    fs::write("seed-result.txt", &text).unwrap();
    assert_eq!(fs::read_to_string("seed-result.txt").unwrap(), text);
    println!("[M15:RUST:SEED] std=ok value={value}");
}
