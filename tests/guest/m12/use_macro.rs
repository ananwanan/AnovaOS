// SPDX-License-Identifier: Apache-2.0
//! Require the compiler to load and invoke the guest-built procedural macro shared object.
fn main() {
    assert_eq!(m12macro::answer!(), 42);
    println!("[M12:MACRO] loaded shared object and expanded answer=42");
}
