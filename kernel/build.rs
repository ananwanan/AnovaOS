// SPDX-License-Identifier: Apache-2.0
//! Reject host-target kernel builds and select the kernel-only linker script.
use std::{env, path::PathBuf};

fn main() {
    assert_eq!(
        env::var("TARGET").unwrap(),
        "x86_64-unknown-none",
        "The kernel requires x86_64-unknown-none; use cargo xtask build"
    );
    let script =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../linker/x86_64.ld");
    println!("cargo:rerun-if-changed={}", script.display());
    println!(
        "cargo:rustc-link-arg-bin=anova-kernel=-T{}",
        script.display()
    );
    println!("cargo:rustc-link-arg-bin=anova-kernel=--build-id=none");
    println!("cargo:rustc-link-arg-bin=anova-kernel=-zmax-page-size=0x1000");
}
