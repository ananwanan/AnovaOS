// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Small local dependency proving the SDK template builds offline with a locked graph.
pub fn message() -> &'static [u8] {
    b"[M13:RUST] native SDK v1 offline dependency\n"
}
