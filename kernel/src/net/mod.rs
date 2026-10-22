// SPDX-License-Identifier: Apache-2.0
//! Integrate pinned smoltcp with bounded IPv4 sockets and the BSP's monotonic scheduler clock.
//! VFS open descriptions own opaque socket IDs; no Linux layouts enter this protocol layer.
use crate::{arch::x86_64::apic, drivers::network::Network};
use alloc::vec::Vec;
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage},
    socket::{tcp, udp},
    time::{Duration, Instant},
    wire::{EthernetAddress, IpAddress, IpCidr, IpEndpoint, IpListenEndpoint, Ipv4Address},
};
pub const LIMIT: usize = 8;
pub const AGAIN: i64 = -11;
fn buffer<T: Clone>(len: usize, value: T) -> Result<Vec<T>, i64> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).map_err(|_| -12)?;
    v.resize(len, value);
    Ok(v)
}
fn now() -> Instant {
    Instant::from_millis(apic::ticks().saturating_mul(10).min(i64::MAX as u64) as i64)
}
#[derive(Clone, Copy)]
struct Entry {
    handle: SocketHandle,
    datagram: bool,
    bound: IpListenEndpoint,
    peer: Option<IpEndpoint>,
    connecting: bool,
    connected: bool,
    error: i64,
    receive_closed: bool,
    send_closed: bool,
    receive_timeout: Option<u64>,
    send_timeout: Option<u64>,
    started: u64,
}
pub struct Stack {
    device: Network,
    interface: Interface,
    sockets: SocketSet<'static>,
    entries: Vec<Option<Entry>>,
    port: u16,
    retired: Vec<(SocketHandle, u64)>,
}
impl Stack {
    pub fn attach() -> Result<Option<Self>, i64> {
        let Some(mut device) = Network::attach()? else {
            return Ok(None);
        };
        let mut config = Config::new(EthernetAddress(device.mac).into());
        let mut seed = [0; 8];
        crate::task::network_seed(&mut seed)?;
        config.random_seed = u64::from_le_bytes(seed);
        let mut interface = Interface::new(config, &mut device, now());
        interface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(IpAddress::v4(10, 0, 2, 15), 24))
                .expect("One IPv4 address fits");
        });
        interface
            .routes_mut()
            .add_default_ipv4_route(Ipv4Address::new(10, 0, 2, 2))
            .map_err(|_| -12)?;
        let mut storage = Vec::new();
        storage.try_reserve_exact(LIMIT * 2).map_err(|_| -12)?;
        storage.resize_with(LIMIT * 2, || SocketStorage::EMPTY);
        let mut entries = Vec::new();
        entries.try_reserve_exact(LIMIT).map_err(|_| -12)?;
        entries.resize_with(LIMIT, || None);
        let mut retired = Vec::new();
        retired.try_reserve_exact(LIMIT * 2).map_err(|_| -12)?;
        Ok(Some(Self {
            device,
            interface,
            sockets: SocketSet::new(storage),
            entries,
            port: 49152,
            retired,
        }))
    }
    pub fn poll(&mut self) {
        self.interface
            .poll(now(), &mut self.device, &mut self.sockets);
        let mut index = 0;
        while index < self.retired.len() {
            let (handle, deadline) = self.retired[index];
            if !self.sockets.get::<tcp::Socket>(handle).is_open() || apic::ticks() >= deadline {
                self.sockets.remove(handle);
                self.retired.swap_remove(index);
            } else {
                index += 1;
            }
        }
        for entry in self.entries.iter_mut().flatten().filter(|e| !e.datagram) {
            let socket = self.sockets.get_mut::<tcp::Socket>(entry.handle);
            if entry.connecting {
                if socket.may_send() {
                    entry.connecting = false;
                    entry.connected = true;
                    // Establishment stays bounded at three seconds; application-level transfer
                    // deadlines must be able to expire before an idle established TCP transport.
                    socket.set_timeout(Some(Duration::from_secs(30)));
                } else if !socket.is_open() {
                    entry.connecting = false;
                    entry.error = if apic::ticks().saturating_sub(entry.started) >= 300 {
                        -110
                    } else {
                        -111
                    };
                }
            }
        }
    }
    pub fn stop(mut self) -> Result<(), i64> {
        self.device.stop()
    }
    pub fn collect(&mut self, mut live: impl FnMut(usize) -> bool) {
        for id in 0..LIMIT {
            if self.entries[id].is_some() && !live(id) {
                let entry = self.entries[id].take().unwrap();
                if entry.datagram || !entry.connected {
                    self.sockets.remove(entry.handle);
                } else {
                    self.sockets.get_mut::<tcp::Socket>(entry.handle).close();
                    self.retired
                        .push((entry.handle, apic::ticks().saturating_add(500)));
                }
            }
        }
    }
    pub fn remove(&mut self, id: usize) {
        if let Some(e) = self.entries[id].take() {
            self.sockets.remove(e.handle);
        }
    }
    pub fn create(&mut self, datagram: bool) -> Result<usize, i64> {
        if self.sockets.iter().count() >= LIMIT * 2 {
            return Err(-24);
        }
        let id = self.entries.iter().position(Option::is_none).ok_or(-24)?;
        let handle = if datagram {
            let rx =
                udp::PacketBuffer::new(buffer(8, udp::PacketMetadata::EMPTY)?, buffer(8192, 0u8)?);
            let tx =
                udp::PacketBuffer::new(buffer(8, udp::PacketMetadata::EMPTY)?, buffer(8192, 0u8)?);
            self.sockets.add(udp::Socket::new(rx, tx))
        } else {
            let mut socket = tcp::Socket::new(
                tcp::SocketBuffer::new(buffer(8192, 0u8)?),
                tcp::SocketBuffer::new(buffer(8192, 0u8)?),
            );
            socket.set_timeout(Some(Duration::from_secs(3)));
            self.sockets.add(socket)
        };
        self.entries[id] = Some(Entry {
            handle,
            datagram,
            bound: IpListenEndpoint::from(0),
            peer: None,
            connecting: false,
            connected: false,
            error: 0,
            receive_closed: false,
            send_closed: false,
            receive_timeout: None,
            send_timeout: None,
            started: 0,
        });
        Ok(id)
    }
    fn entry(&self, id: usize) -> Result<&Entry, i64> {
        self.entries.get(id).and_then(Option::as_ref).ok_or(-9)
    }
    fn ephemeral(&mut self) -> u16 {
        for _ in 0..=LIMIT {
            let port = self.port;
            self.port = if self.port == 65535 {
                49152
            } else {
                self.port + 1
            };
            if !self
                .entries
                .iter()
                .flatten()
                .any(|entry| entry.bound.port == port)
            {
                return port;
            }
        }
        unreachable!("Eight sockets cannot occupy nine distinct ephemeral candidates")
    }
    pub fn bind(&mut self, id: usize, endpoint: IpListenEndpoint) -> Result<(), i64> {
        if endpoint
            .addr
            .is_some_and(|a| a != IpAddress::v4(10, 0, 2, 15))
        {
            return Err(-99);
        }
        if self.entries.iter().enumerate().any(|(k, e)| {
            k != id
                && e.as_ref()
                    .is_some_and(|e| e.bound.port == endpoint.port && endpoint.port != 0)
        }) {
            return Err(-98);
        }
        let e = self.entry(id)?;
        if e.bound.port != 0 || e.connected || e.connecting {
            return Err(-22);
        }
        let endpoint = if endpoint.port == 0 {
            IpListenEndpoint {
                port: self.ephemeral(),
                ..endpoint
            }
        } else {
            endpoint
        };
        let e = self.entries[id].as_mut().unwrap();
        if e.datagram {
            self.sockets
                .get_mut::<udp::Socket>(e.handle)
                .bind(endpoint)
                .map_err(|_| -98)?;
        }
        e.bound = endpoint;
        Ok(())
    }
    pub fn connect(&mut self, id: usize, peer: IpEndpoint) -> Result<(), i64> {
        let e = self.entry(id)?;
        if e.connected && !e.datagram {
            return Err(-106);
        }
        if e.connecting {
            return Err(-114);
        }
        if peer.port == 0 || peer.addr.is_unspecified() {
            return Err(-22);
        }
        if e.bound.port == 0 {
            self.bind(id, IpListenEndpoint::from(0))?;
        }
        let e = self.entries[id].as_mut().unwrap();
        e.peer = Some(peer);
        if e.datagram {
            e.connected = true;
            return Ok(());
        }
        self.sockets
            .get_mut::<tcp::Socket>(e.handle)
            .connect(self.interface.context(), peer, e.bound)
            .map_err(|_| -22)?;
        e.started = apic::ticks();
        e.connecting = true;
        Err(-115)
    }
    pub fn completion(&self, id: usize) -> Result<(), i64> {
        let e = self.entry(id)?;
        if e.error != 0 {
            return Err(e.error);
        }
        if e.connecting {
            return Err(AGAIN);
        }
        if e.connected { Ok(()) } else { Err(-107) }
    }
    pub fn endpoints(&self, id: usize) -> Result<(IpListenEndpoint, Option<IpEndpoint>), i64> {
        let e = self.entry(id)?;
        Ok((e.bound, e.peer))
    }
    pub fn take_error(&mut self, id: usize) -> Result<i64, i64> {
        let e = self
            .entries
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(-9)?;
        let error = e.error;
        e.error = 0;
        Ok(error)
    }
    pub fn readiness(&self, id: usize) -> Result<(bool, bool, bool, bool), i64> {
        let e = self.entry(id)?;
        if e.datagram {
            let s = self.sockets.get::<udp::Socket>(e.handle);
            return Ok((
                e.receive_closed || s.can_recv(),
                !e.send_closed && s.can_send(),
                false,
                e.error != 0,
            ));
        }
        let s = self.sockets.get::<tcp::Socket>(e.handle);
        Ok((
            e.receive_closed || s.can_recv() || e.connected && !s.may_recv(),
            e.connected && s.can_send() || e.error != 0,
            e.connected && !s.is_active(),
            e.error != 0,
        ))
    }
    pub fn send(
        &mut self,
        id: usize,
        bytes: &[u8],
        peer: Option<IpEndpoint>,
    ) -> Result<usize, i64> {
        let e = self.entry(id)?;
        if e.send_closed {
            return Err(-32);
        }
        let datagram = e.datagram;
        if datagram && e.bound.port == 0 {
            self.bind(id, IpListenEndpoint::from(0))?;
        }
        let e = self.entry(id)?;
        if datagram {
            if bytes.len() > 1472 {
                return Err(-90);
            }
            let peer = peer.or(e.peer).ok_or(-89)?;
            self.sockets
                .get_mut::<udp::Socket>(e.handle)
                .send_slice(bytes, peer)
                .map_err(|_| AGAIN)?;
            Ok(bytes.len())
        } else {
            if peer.is_some() {
                return Err(-106);
            }
            if e.error != 0 {
                return Err(e.error);
            }
            let s = self.sockets.get_mut::<tcp::Socket>(e.handle);
            if !s.may_send() {
                return Err(-32);
            }
            s.send_slice(bytes).map_err(|_| AGAIN).and_then(|n| {
                if n == 0 && !bytes.is_empty() {
                    Err(AGAIN)
                } else {
                    Ok(n)
                }
            })
        }
    }
    pub fn receive(
        &mut self,
        id: usize,
        bytes: &mut [u8],
    ) -> Result<(usize, Option<IpEndpoint>), i64> {
        let e = *self.entry(id)?;
        if e.receive_closed {
            return Ok((0, e.peer));
        }
        if e.datagram {
            let s = self.sockets.get_mut::<udp::Socket>(e.handle);
            for _ in 0..8 {
                let (data, meta) = s.recv().map_err(|_| AGAIN)?;
                if e.peer.is_some_and(|peer| peer != meta.endpoint) {
                    continue;
                }
                let n = bytes.len().min(data.len());
                bytes[..n].copy_from_slice(&data[..n]);
                return Ok((n, Some(meta.endpoint)));
            }
            Err(AGAIN)
        } else {
            if e.error != 0 {
                return Err(e.error);
            }
            let peer = e.peer;
            let s = self.sockets.get_mut::<tcp::Socket>(e.handle);
            if !s.can_recv() {
                return if !s.may_recv() && e.connected {
                    Ok((0, peer))
                } else {
                    Err(AGAIN)
                };
            }
            s.recv_slice(bytes).map(|n| (n, peer)).map_err(|_| AGAIN)
        }
    }
    pub fn timeout(&mut self, id: usize, writing: bool, ticks: Option<u64>) -> Result<(), i64> {
        let e = self
            .entries
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(-9)?;
        if writing {
            e.send_timeout = ticks;
        } else {
            e.receive_timeout = ticks;
        }
        Ok(())
    }
    pub fn get_timeout(&self, id: usize, writing: bool) -> Option<u64> {
        self.entry(id).ok().and_then(|e| {
            if writing {
                e.send_timeout
            } else {
                e.receive_timeout
            }
        })
    }
    pub fn shutdown(&mut self, id: usize, how: u64) -> Result<(), i64> {
        if how > 2 {
            return Err(-22);
        }
        let e = self
            .entries
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or(-9)?;
        if !e.connected {
            return Err(-107);
        }
        if how != 1 {
            e.receive_closed = true;
        }
        if how != 0 {
            e.send_closed = true;
            if !e.datagram {
                self.sockets.get_mut::<tcp::Socket>(e.handle).close();
            }
        }
        Ok(())
    }
}
