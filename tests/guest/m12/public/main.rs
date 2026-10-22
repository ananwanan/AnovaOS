// SPDX-License-Identifier: Apache-2.0
//! Execute public registry and full-revision Git copies of itoa compiled by guest rustc.
fn main() {
    let mut registry = itoa::Buffer::new();
    let mut git = itoa_git::Buffer::new();
    assert_eq!(registry.format(42), git.format(42));
    println!("[M12:PUBLIC] crates.io=true github=true answer=42");
}
