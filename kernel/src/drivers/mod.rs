// SPDX-License-Identifier: Apache-2.0
//! Keep hardware discovery separate from the VFS and independent of userspace.
mod pci;

/// Perform a best-effort, read-only PCI device probe; no BARs or DMA are enabled.
pub fn discover() {
    pci::probe();
}
