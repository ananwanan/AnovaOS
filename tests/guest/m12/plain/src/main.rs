// SPDX-License-Identifier: Apache-2.0
//! Validate the dependency-free Cargo stage with a real executable produced in guest.
fn main() {
    let values = [19, 23].to_vec();
    assert_eq!(values.iter().sum::<i32>(), 42);
    println!("[M12:PLAIN] dependency-free Cargo answer=42");
}
