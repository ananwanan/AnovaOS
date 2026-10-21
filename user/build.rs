// SPDX-License-Identifier: Apache-2.0
//! Rebuild native app binaries when their external user-only linker script changes.
fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let linker = manifest
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("linker/user-x86_64.ld");
    println!("cargo:rerun-if-changed={}", linker.display());
    assert_eq!(
        std::env::var("TARGET").unwrap(),
        "x86_64-unknown-linux-gnu",
        "Use cargo xtask apps for user target/linker settings"
    );
}
