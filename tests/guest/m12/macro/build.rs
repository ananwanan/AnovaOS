// SPDX-License-Identifier: Apache-2.0
//! Generate macro input from a real host build dependency and expose the matching environment.
fn main() {
    let answer = m12_macro_support::answer();
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(
        output.join("generated.rs"),
        format!("const BUILD_ANSWER: u32 = {answer};\n"),
    )
    .unwrap();
    println!("cargo:rustc-env=M12_MACRO_BUILD={answer}");
    println!("cargo:rerun-if-changed=build.rs");
}
