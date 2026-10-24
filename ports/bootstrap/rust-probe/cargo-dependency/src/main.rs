// SPDX-License-Identifier: Apache-2.0
//! Require Cargo's audited curl dependency; the accepted offline vendor cannot resolve it.
fn main() {
    let _client = curl::easy::Easy::new();
}
