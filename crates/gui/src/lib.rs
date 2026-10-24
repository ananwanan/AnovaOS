//! Compose bounded private RGB surfaces and validate the experimental M14.3 GUI protocol.
//!
//! The kernel authenticates transport client identities and owns process/display isolation;
//! this allocation-free crate owns user-space stacking, decoration, focus and input routing.
//! All pixel buffers are private copies. Upload acknowledgement releases the caller's input;
//! each chunk commits independently, with no atomic multi-chunk frame or vblank guarantee.
//! This internal wire version is not the future M14.4 public C/C++/Rust SDK ABI.
//!
//! SPDX-License-Identifier: Apache-2.0

#![no_std]

mod policy;
pub mod transport;
mod wire;

pub use policy::*;
pub use wire::*;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
