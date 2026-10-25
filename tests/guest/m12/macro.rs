// SPDX-License-Identifier: Apache-2.0
//! Export a small real host procedural macro for dynamic loading by the matching guest compiler.
extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro]
pub fn answer(_: TokenStream) -> TokenStream {
    "42".parse().unwrap()
}
