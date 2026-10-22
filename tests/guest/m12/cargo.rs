// SPDX-License-Identifier: Apache-2.0
//! Prove that a checksum-locked vendored registry crate is compiled and linked inside the guest.
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
unsafe extern "C" {
    fn m12_ffi_answer() -> i32;
}
fn main() {
    // SAFETY: The guest-built C library defines this exact no-argument int32 ABI and returns a constant.
    assert_eq!(unsafe { m12_ffi_answer() }, 84);
    println!("[M12:FFI] build.rs gcc and static C linkage answer=84");
    assert_eq!(GENERATED, 42);
    assert_eq!(env!("M12_BUILD"), "guest-build-script");
    let mut buffer = itoa::Buffer::new();
    let value = buffer.format(42);
    assert_eq!(value, "42");
    std::fs::write("cargo-result.txt", value).unwrap();
    println!("[M12:CARGO] offline vendor value={value}");
}
