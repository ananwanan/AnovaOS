// SPDX-License-Identifier: Apache-2.0
//! Expand a Cargo-built macro using its dependency, generated source and build-script environment.
use proc_macro::TokenStream;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[proc_macro]
pub fn answer(input: TokenStream) -> TokenStream {
    if !input.is_empty() {
        return "compile_error!(\"m12-macro expects no tokens\")"
            .parse()
            .unwrap();
    }
    let answer = m12_macro_support::answer();
    assert_eq!(answer, BUILD_ANSWER.to_string());
    assert_eq!(env!("M12_MACRO_BUILD"), answer);
    answer.parse().unwrap()
}
