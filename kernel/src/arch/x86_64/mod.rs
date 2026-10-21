// SPDX-License-Identifier: Apache-2.0
//! Select the x86_64 CPU, paging, interrupt and user-transition implementations.
pub mod acpi;
pub mod apic;
pub mod cpu;
pub mod paging;
pub mod traps;
pub mod user;
