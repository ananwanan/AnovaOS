// SPDX-License-Identifier: Apache-2.0
//! Translate Linux IPv4 networking and bounded AF_UNIX packet pairs with checked interruptible I/O.
//! Validate destination buffers before dequeuing data; retain only syscall frames while blocked.
use super::*;
use smoltcp::wire::{IpAddress, IpEndpoint, IpListenEndpoint};
impl Kernel {
    pub(in crate::task) fn socket_ready(&self, i: usize, f: &TrapFrame) -> bool {
        let p = self.process(i);
        if self.fs.is_packet(&p.files, f.rdi) {
            return self.fs.ready(&p.files, f.rdi, matches!(f.rax, 1 | 44));
        }
        if p.network_deadline.is_some_and(|d| apic::ticks() >= d) {
            return true;
        }
        let Some(net) = self.network.as_ref() else {
            return true;
        };
        let Some(id) = self.fs.socket_id(&p.files, f.rdi).ok().flatten() else {
            return true;
        };
        if f.rax == 42 {
            return net.completion(id) != Err(crate::net::AGAIN);
        }
        net.readiness(id).map_or(true, |(read, write, hup, error)| {
            error || hup || if matches!(f.rax, 1 | 44) { write } else { read }
        })
    }
    pub(super) fn socket_syscall(&mut self, i: usize, f: &TrapFrame) -> i64 {
        let result = self.socket_call(i, f).unwrap_or_else(|e| e);
        if result == crate::net::AGAIN {
            let p = self.process(i);
            let nonblocking = self.fs.nonblocking(&p.files, f.rdi).unwrap_or(false)
                || matches!(f.rax, 44 | 45) && f.r10 & 0x40 != 0;
            if nonblocking {
                self.process_mut(i).network_deadline = None;
                return if f.rax == 42 { -115 } else { -11 };
            }
            if p.network_deadline.is_some_and(|d| apic::ticks() >= d) {
                self.process_mut(i).network_deadline = None;
                return if f.rax == 42 { -110 } else { -11 };
            }
            self.process_mut(i).io = Some(*f);
            self.policy.block(i);
            self.metrics.io_blocks += 1;
            WOULD_BLOCK
        } else {
            self.process_mut(i).network_deadline = None;
            if result == -32 && (f.rax == 1 || f.rax == 44 && f.r10 & 0x4000 == 0) {
                self.queue_signal(i, 13);
            }
            result
        }
    }
    fn socket_call(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if f.rax == 53 {
            if f.rdi != 1 {
                return Err(-97);
            }
            if f.rsi & !(0xf | 0x800 | 0x80000) != 0 {
                return Err(-22);
            }
            if f.rsi & 0xf != 5 {
                return Err(-94);
            }
            if f.rdx != 0 {
                return Err(-93);
            }
            let p = self.processes[i].as_mut().unwrap();
            let space = &p.image.as_ref().unwrap().space;
            if !space.accessible(UserAddr(f.r10), 8, true, false) {
                return Err(-14);
            }
            let pair = self.fs.packet_pair(&mut p.files).map_err(errno)?;
            let result = space.with_active(|| -> Result<(), i64> {
                let mut bytes = [0; 8];
                for (k, fd) in pair.iter().enumerate() {
                    self.fs
                        .set_nonblocking(&p.files, *fd, f.rsi & 0x800 != 0)
                        .map_err(errno)?;
                    p.files
                        .set_close_on_exec(*fd, f.rsi & 0x80000 != 0)
                        .map_err(errno)?;
                    bytes[k * 4..k * 4 + 4].copy_from_slice(&(*fd as u32).to_le_bytes());
                }
                uaccess::to_user(space, UserAddr(f.r10), &bytes).map_err(errno)
            });
            if let Err(error) = result {
                for fd in pair {
                    let _ = self.fs.close(&mut p.files, fd);
                }
                return Err(error);
            }
            return Ok(0);
        }
        if self.fs.is_packet(&self.process(i).files, f.rdi) {
            return self.packet_call(i, f);
        }
        if matches!(f.rax, 43 | 46 | 47 | 50) {
            return Err(-38);
        }
        if f.rax == 41 {
            if f.rdi != 2 {
                return Err(-97);
            }
            if !matches!(f.rsi & 15, 1 | 2) || f.rsi & !(15 | 0x800 | 0x80000) != 0 {
                return Err(-22);
            }
            let datagram = f.rsi & 15 == 2;
            if f.rdx != 0 && f.rdx != if datagram { 17 } else { 6 } {
                return Err(-93);
            }
            let net = self.network.as_mut().ok_or(-100)?;
            net.collect(|id| self.fs.socket_live(id));
            let id = net.create(datagram)?;
            let fd = match self
                .fs
                .socket(&mut self.processes[i].as_mut().unwrap().files, id)
            {
                Ok(fd) => fd,
                Err(e) => {
                    net.remove(id);
                    return Err(errno(e));
                }
            };
            let files = &mut self.processes[i].as_mut().unwrap().files;
            self.fs
                .set_nonblocking(files, fd, f.rsi & 0x800 != 0)
                .map_err(errno)?;
            files
                .set_close_on_exec(fd, f.rsi & 0x80000 != 0)
                .map_err(errno)?;
            return Ok(fd as i64);
        }
        let active = self.network_deadline_for(i, f);
        let p = self.processes[i].as_ref().unwrap();
        let id = self
            .fs
            .socket_id(&p.files, f.rdi)
            .map_err(errno)?
            .ok_or(-88)?;
        let space = &p.image.as_ref().unwrap().space;
        space.with_active(|| -> Result<i64, i64> {
            let net = self.network.as_mut().ok_or(-100)?;
            match f.rax {
                42 | 49 => {
                    let endpoint = address(space, f.rsi, f.rdx)?;
                    if f.rax == 49 {
                        net.bind(id, endpoint)?;
                        return Ok(0);
                    }
                    if active {
                        net.completion(id)?;
                        return Ok(0);
                    }
                    let peer = IpEndpoint::new(endpoint.addr.ok_or(-22)?, endpoint.port);
                    match net.connect(id, peer) {
                        Ok(()) => Ok(0),
                        Err(-115) => Err(-11),
                        Err(e) => Err(e),
                    }
                }
                0 | 1 | 44 | 45 => {
                    let writing = matches!(f.rax, 1 | 44);
                    if matches!(f.rax, 44 | 45) && f.r10 & !(0x40 | 0x4000) != 0 {
                        return Err(-95);
                    }
                    let length = usize::try_from(f.rdx).map_err(|_| -22)?.min(8192);
                    if !space.accessible(UserAddr(f.rsi), length, !writing, false) {
                        return Err(-14);
                    }
                    let mut bytes = [0; 8192];
                    if writing {
                        uaccess::from_user(space, UserAddr(f.rsi), &mut bytes[..length])
                            .map_err(errno)?;
                        let peer = if f.rax == 44 && f.r8 != 0 {
                            let e = address(space, f.r8, f.r9)?;
                            Some(IpEndpoint::new(e.addr.ok_or(-22)?, e.port))
                        } else {
                            None
                        };
                        net.send(id, &bytes[..length], peer).map(|n| n as i64)
                    } else {
                        if f.rax == 45 && f.r8 != 0 {
                            validate_address_output(space, f.r8, f.r9)?;
                        }
                        if length == 0 {
                            return Ok(0);
                        }
                        let (n, peer) = net.receive(id, &mut bytes[..length])?;
                        uaccess::to_user(space, UserAddr(f.rsi), &bytes[..n]).map_err(errno)?;
                        if f.rax == 45 && f.r8 != 0 {
                            write_address(
                                space,
                                f.r8,
                                f.r9,
                                peer.map(IpListenEndpoint::from)
                                    .unwrap_or(IpListenEndpoint::from(0)),
                            )?;
                        }
                        Ok(n as i64)
                    }
                }
                51 | 52 => {
                    validate_address_output(space, f.rsi, f.rdx)?;
                    let (local, peer) = net.endpoints(id)?;
                    let endpoint = if f.rax == 51 {
                        IpListenEndpoint {
                            addr: Some(local.addr.unwrap_or(IpAddress::v4(10, 0, 2, 15))),
                            ..local
                        }
                    } else {
                        IpListenEndpoint::from(peer.ok_or(-107)?)
                    };
                    write_address(space, f.rsi, f.rdx, endpoint)?;
                    Ok(0)
                }
                48 => {
                    net.shutdown(id, f.rsi)?;
                    Ok(0)
                }
                54 => {
                    if f.rsi != 1 {
                        return Err(-92);
                    }
                    match f.rdx {
                        20 | 21 => {
                            if f.r8 != 16 {
                                return Err(-22);
                            }
                            let mut bytes = [0; 16];
                            uaccess::from_user(space, UserAddr(f.r10), &mut bytes)
                                .map_err(errno)?;
                            let seconds = i64::from_le_bytes(bytes[..8].try_into().unwrap());
                            let micros = i64::from_le_bytes(bytes[8..].try_into().unwrap());
                            if seconds < 0 || !(0..1_000_000).contains(&micros) {
                                return Err(-33);
                            }
                            let ticks = (seconds as u64)
                                .checked_mul(100)
                                .and_then(|s| s.checked_add((micros as u64).div_ceil(10000)))
                                .ok_or(-22)?;
                            net.timeout(
                                id,
                                f.rdx == 21,
                                if ticks == 0 { None } else { Some(ticks) },
                            )?;
                            Ok(0)
                        }
                        _ => Err(-92),
                    }
                }
                55 => {
                    if f.rsi != 1 || f.rdx != 4 {
                        return Err(-92);
                    }
                    let mut len = [0; 4];
                    uaccess::from_user(space, UserAddr(f.r8), &mut len).map_err(errno)?;
                    if u32::from_le_bytes(len) < 4 {
                        return Err(-22);
                    }
                    if !space.accessible(UserAddr(f.r10), 4, true, false)
                        || !space.accessible(UserAddr(f.r8), 4, true, false)
                    {
                        return Err(-14);
                    }
                    let error = (-net.take_error(id)?) as i32;
                    uaccess::to_user(space, UserAddr(f.r10), &error.to_le_bytes())
                        .map_err(errno)?;
                    uaccess::to_user(space, UserAddr(f.r8), &4u32.to_le_bytes()).map_err(errno)?;
                    Ok(0)
                }
                _ => Err(-95),
            }
        })
    }
    fn packet_call(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if !matches!(f.rax, 0 | 1 | 44 | 45) {
            return Err(-95);
        }
        let writing = matches!(f.rax, 1 | 44);
        if matches!(f.rax, 44 | 45) && (f.r10 & !(0x40 | 0x4000) != 0 || f.r8 != 0) {
            return Err(-95);
        }
        if writing && f.rdx > anova_vfs::PACKET_BYTES as u64 {
            return Err(-90);
        }
        let length = f.rdx.min(anova_vfs::PACKET_BYTES as u64) as usize;
        let p = self.processes[i].as_ref().unwrap();
        let space = &p.image.as_ref().unwrap().space;
        space.with_active(|| {
            if !space.accessible(UserAddr(f.rsi), length, !writing, false) {
                return Err(-14);
            }
            let mut bytes = [0; anova_vfs::PACKET_BYTES];
            if writing {
                uaccess::from_user(space, UserAddr(f.rsi), &mut bytes[..length]).map_err(errno)?;
                self.fs
                    .write(&p.files, f.rdi, &bytes[..length])
                    .map(|n| n as i64)
                    .map_err(errno)
            } else {
                let n = self
                    .fs
                    .read(&p.files, f.rdi, &mut bytes[..length])
                    .map_err(errno)?;
                uaccess::to_user(space, UserAddr(f.rsi), &bytes[..n]).map_err(errno)?;
                Ok(n as i64)
            }
        })
    }
    fn network_deadline_for(&mut self, i: usize, f: &TrapFrame) -> bool {
        if f.rax == 42 {
            let active = self.process(i).network_deadline.is_some();
            if !active {
                self.process_mut(i).network_deadline = Some(apic::ticks().saturating_add(350));
            }
            return active;
        }
        if self.process(i).network_deadline.is_none() && matches!(f.rax, 0 | 1 | 44 | 45) {
            let p = self.process(i);
            if let Some(id) = self.fs.socket_id(&p.files, f.rdi).ok().flatten()
                && let Some(ticks) = self
                    .network
                    .as_ref()
                    .and_then(|n| n.get_timeout(id, matches!(f.rax, 1 | 44)))
            {
                self.process_mut(i).network_deadline = Some(apic::ticks().saturating_add(ticks));
            }
        }
        false
    }
}
fn address(space: &mm::user::UserSpace, at: u64, len: u64) -> Result<IpListenEndpoint, i64> {
    if len != 16 {
        return Err(-22);
    }
    let mut b = [0; 16];
    uaccess::from_user(space, UserAddr(at), &mut b).map_err(errno)?;
    if u16::from_le_bytes(b[..2].try_into().unwrap()) != 2 {
        return Err(-97);
    }
    let port = u16::from_be_bytes(b[2..4].try_into().unwrap());
    let addr = if b[4..8] == [0; 4] {
        None
    } else {
        Some(IpAddress::v4(b[4], b[5], b[6], b[7]))
    };
    Ok(IpListenEndpoint { addr, port })
}
fn validate_address_output(space: &mm::user::UserSpace, at: u64, len: u64) -> Result<(), i64> {
    let mut b = [0; 4];
    uaccess::from_user(space, UserAddr(len), &mut b).map_err(errno)?;
    let n = (u32::from_le_bytes(b) as usize).min(16);
    if !space.accessible(UserAddr(len), 4, true, false)
        || !space.accessible(UserAddr(at), n, true, false)
    {
        return Err(-14);
    }
    Ok(())
}
fn write_address(
    space: &mm::user::UserSpace,
    at: u64,
    len: u64,
    endpoint: IpListenEndpoint,
) -> Result<(), i64> {
    let mut size = [0; 4];
    uaccess::from_user(space, UserAddr(len), &mut size).map_err(errno)?;
    let n = (u32::from_le_bytes(size) as usize).min(16);
    let mut b = [0; 16];
    b[..2].copy_from_slice(&2u16.to_le_bytes());
    b[2..4].copy_from_slice(&endpoint.port.to_be_bytes());
    if let Some(IpAddress::Ipv4(ip)) = endpoint.addr {
        b[4..8].copy_from_slice(&ip.octets());
    }
    uaccess::to_user(space, UserAddr(at), &b[..n]).map_err(errno)?;
    uaccess::to_user(space, UserAddr(len), &16u32.to_le_bytes()).map_err(errno)?;
    Ok(())
}
