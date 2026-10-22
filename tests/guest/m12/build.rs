// SPDX-License-Identifier: Apache-2.0
//! Generate and consume an OUT_DIR source file inside the guest Cargo build-script process.
fn main() {
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let object = output.join("helper.o");
    let archive = output.join("libm12ffi.a");
    assert!(
        std::process::Command::new("/tc/bin/gcc")
            .args(["--sysroot=/tc", "-O2", "-fPIC", "-c", "helper.c", "-o"])
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("/tc/bin/ar")
            .arg("rcs")
            .arg(&archive)
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=m12ffi");
    std::fs::write(output.join("generated.rs"), "const GENERATED: u32 = 42;\n").unwrap();
    println!("cargo:rustc-env=M12_BUILD=guest-build-script");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=helper.c");
}
