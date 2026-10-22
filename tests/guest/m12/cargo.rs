// SPDX-License-Identifier: Apache-2.0
//! Verify vendored code, C FFI and a Cargo-built procedural macro with its host dependency.
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
unsafe extern "C" {
    fn m12_ffi_answer() -> i32;
}
fn main() {
    let macro_answer: u32 = m12_macro::answer!();
    let expected = std::env::args()
        .nth(1)
        .map(|value| value.parse::<u32>().unwrap())
        .unwrap_or(42);
    assert_eq!(macro_answer, expected);
    println!("[M12:CARGO_MACRO] answer={macro_answer}");
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
