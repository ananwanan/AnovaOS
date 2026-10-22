// SPDX-License-Identifier: Apache-2.0
//! Execute independently downloaded registry and revision-locked Git dependencies in the guest.
fn main() {
    let answer = m12_net_dep::answer() + m12_git_dep::answer();
    assert_eq!(answer, 42);
    println!("[M12:ONLINE] registry=true git=true answer={answer}");
}
