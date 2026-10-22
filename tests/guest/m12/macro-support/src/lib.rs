// SPDX-License-Identifier: Apache-2.0
//! Supply a value whose source edit must rebuild the host macro and its downstream application.
pub fn answer() -> &'static str {
    "42"
}
