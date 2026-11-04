// SPDX-License-Identifier: Apache-2.0
//! Check exact parent/child identity before nonblocking native status copying or termination.
//! No global task authority, Linux threads, descendants or display privileges are granted.

use super::*;

impl Kernel {
    pub(super) fn process_control(&mut self, caller: usize, frame: &TrapFrame) -> i64 {
        let Some(operation) = anova_abi::process::decode(frame.rdi) else {
            return INVALID_ARGUMENT;
        };
        if operation == anova_abi::process::STOP && frame.rdx != 0 {
            return INVALID_ARGUMENT;
        }
        let child = match self.policy.child(caller, frame.rsi) {
            Ok(child) => child,
            Err(error) => return error,
        };
        let entry = self.policy.entry(child);
        let zombie = matches!(entry.state, State::Zombie(_));
        if !zombie && self.process(child).image.as_ref().unwrap().linux {
            return UNSUPPORTED;
        }
        match operation {
            anova_abi::process::POLL if zombie => {
                // wait copies first and reaps only after successful checked user access.
                self.wait(caller, frame.rsi, UserAddr(frame.rdx))
                    .expect("Observed zombie cannot block")
            }
            anova_abi::process::POLL => WOULD_BLOCK,
            anova_abi::process::STOP if zombie => 0,
            anova_abi::process::STOP => {
                self.finish(child, Outcome::Signal(15));
                0
            }
            _ => INVALID_ARGUMENT,
        }
    }
}
