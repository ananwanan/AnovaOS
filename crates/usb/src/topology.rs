// SPDX-License-Identifier: Apache-2.0
//! Bound USB2 hub descriptors, default TT mode, routes and inherited translator identities.
//! No host registers, device I/O or allocation is performed by these checked records.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hub {
    pub ports: u8,
    pub power_ms: u16,
    pub think_time: u8,
    pub multi_tt: bool,
}
impl Hub {
    /// Validate the default interface; selecting the multi-TT alternate is outside this profile.
    pub fn parse_default(
        bytes: &[u8],
        speed: u8,
        device_protocol: u8,
        interface_protocol: u8,
    ) -> Result<Self, &'static str> {
        if bytes.len() < 9 || bytes[1] != 0x29 {
            return Err("hub-descriptor-header");
        }
        // USB2 11.23.1: high-speed multi-TT capability (device protocol 2) still
        // defaults to single-TT operation (interface protocol 1). Never enable
        // xHCI MTT from capability alone, or without selecting an alternate.
        if !matches!(
            (speed, device_protocol, interface_protocol),
            (1, 0, 0) | (3, 1, 0) | (3, 2, 1)
        ) {
            return Err("hub-default-protocol");
        }
        let ports = bytes[2];
        let length = 7 + 2 * (usize::from(ports) + 1).div_ceil(8);
        if !(1..=15).contains(&ports) || usize::from(bytes[0]) != length || bytes.len() != length {
            return Err("hub-descriptor-ports-length");
        }
        let characteristics = u16::from_le_bytes([bytes[3], bytes[4]]);
        if characteristics & 0xff00 != 0
            || characteristics & 3 == 3
            || (characteristics >> 3) & 3 == 3
        {
            return Err("hub-characteristics");
        }
        Ok(Self {
            ports,
            power_ms: u16::from(bytes[5]) * 2,
            think_time: if speed == 3 {
                ((characteristics >> 5) & 3) as u8
            } else {
                0
            },
            multi_tt: false,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Topology {
    pub root: u8,
    pub route: u32,
    pub depth: u8,
    pub parent: u8,
    pub port: u8,
    pub speed: u8,
    pub tt_slot: u8,
    pub tt_port: u8,
    pub multi_tt: bool,
}
impl Topology {
    pub fn root(root: u8, speed: u8) -> Result<Self, &'static str> {
        if root == 0 || !(1..=4).contains(&speed) {
            return Err("topology-root");
        }
        Ok(Self {
            root,
            speed,
            route: 0,
            depth: 0,
            parent: 0,
            port: root,
            tt_slot: 0,
            tt_port: 0,
            multi_tt: false,
        })
    }
    pub fn child(self, slot: u8, hub: Hub, port: u8, speed: u8) -> Result<Self, &'static str> {
        if self.depth >= 5
            || slot == 0
            || port == 0
            || port > hub.ports
            || port > 15
            || !matches!(self.speed, 1 | 3)
            || !(1..=3).contains(&speed)
            || self.speed == 1 && speed == 3
        {
            return Err("topology-child-bounds");
        }
        let (tt_slot, tt_port, multi_tt) = if speed == 3 {
            (0, 0, false)
        } else if self.speed == 3 {
            (slot, port, hub.multi_tt)
        } else {
            (self.tt_slot, self.tt_port, self.multi_tt)
        };
        Ok(Self {
            root: self.root,
            route: self.route | u32::from(port) << (self.depth * 4),
            depth: self.depth + 1,
            parent: slot,
            port,
            speed,
            tt_slot,
            tt_port,
            multi_tt,
        })
    }
    pub fn below(self, ancestor: Self) -> bool {
        if self.depth > 5 || ancestor.depth > 5 {
            return false;
        }
        let mask = (1u32 << (ancestor.depth * 4)) - 1;
        self.root == ancestor.root
            && self.depth > ancestor.depth
            && self.route & mask == ancestor.route
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DESCRIPTOR: &[u8] = &[9, 0x29, 4, 0x69, 0, 50, 0, 0, 255];
    #[test]
    fn hub_descriptor_checks_lengths_power_and_tt() {
        let h = Hub::parse_default(DESCRIPTOR, 3, 2, 1).unwrap();
        assert_eq!(
            (h.ports, h.power_ms, h.think_time, h.multi_tt),
            (4, 100, 3, false)
        );
        for n in 0..DESCRIPTOR.len() {
            assert!(Hub::parse_default(&DESCRIPTOR[..n], 3, 2, 1).is_err());
        }
        for (at, value) in [(0, 8), (1, 0x2a), (2, 0), (2, 16), (3, 3), (4, 1)] {
            let mut bytes = DESCRIPTOR.to_vec();
            bytes[at] = value;
            assert!(Hub::parse_default(&bytes, 3, 2, 1).is_err());
        }
        assert_eq!(
            Hub::parse_default(DESCRIPTOR, 1, 0, 0).unwrap().think_time,
            0
        );
        assert!(!Hub::parse_default(DESCRIPTOR, 3, 1, 0).unwrap().multi_tt);
        for (speed, device, interface) in [
            (4, 2, 1),
            (1, 1, 0),
            (1, 0, 1),
            (3, 0, 0),
            (3, 1, 1),
            (3, 2, 0),
            (3, 2, 2),
        ] {
            assert!(Hub::parse_default(DESCRIPTOR, speed, device, interface).is_err());
        }
    }
    #[test]
    fn nested_routes_inherit_nearest_tt_and_reject_depth() {
        // Synthetic selected-multi-TT metadata tests inheritance only; the driver
        // deliberately keeps every high-speed hub on its default single-TT interface.
        let h = Hub {
            multi_tt: true,
            ..Hub::parse_default(DESCRIPTOR, 3, 2, 1).unwrap()
        };
        let root = Topology::root(5, 3).unwrap();
        let child = root.child(1, h, 3, 1).unwrap();
        assert_eq!(
            (child.route, child.tt_slot, child.tt_port, child.multi_tt),
            (3, 1, 3, true)
        );
        let grandchild = child.child(2, h, 2, 2).unwrap();
        assert_eq!(
            (grandchild.route, grandchild.tt_slot, grandchild.tt_port),
            (0x23, 1, 3)
        );
        assert!(grandchild.below(child));
        assert!(!child.below(grandchild));
        assert!(!root.child(1, h, 1, 1).unwrap().below(child));
        assert_eq!(root.child(1, h, 2, 3).unwrap().tt_slot, 0);
        let mut current = child;
        for _ in 0..4 {
            current = current.child(2, h, 1, 1).unwrap();
        }
        assert_eq!(current.depth, 5);
        assert!(current.child(2, h, 1, 1).is_err());
        assert!(root.child(1, h, 5, 1).is_err());
        assert!(child.child(2, h, 1, 3).is_err());
    }
}
