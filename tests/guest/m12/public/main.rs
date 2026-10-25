// SPDX-License-Identifier: Apache-2.0
//! Execute public registry and full-revision Git copies of itoa compiled by guest rustc.
fn main() {
    let mut registry = itoa::Buffer::new();
    let mut git = itoa_git::Buffer::new();
    let registry = registry.format(42);
    let git = git.format(42);
    assert_eq!(registry, "42");
    assert_eq!(git, "42");
    println!("[M12:PUBLIC] crates.io=true github=true answer={registry}");
}
