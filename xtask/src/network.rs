// SPDX-License-Identifier: Apache-2.0
//! Run controlled loopback TCP/UDP/DNS peers and exercise real UART socket programs under QEMU.
//! Peers bind explicit loopback ports; guest watchdogs and exact success markers determine acceptance.
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
pub fn attach(cmd: &mut Command) {
    cmd.args([
        "-netdev",
        "user,id=m10,net=10.0.2.0/24",
        "-device",
        "virtio-net-pci,netdev=m10,disable-legacy=on,mac=52:54:00:12:34:56,romfile=",
    ]);
}
struct Peers {
    stop: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
    port: u16,
}
impl Peers {
    fn start() -> Result<Self> {
        // Reserve all peer sockets before returning, so port collisions are explicit failures.
        let tcp = TcpListener::bind(("127.0.0.1", 18080))?;
        let udp = UdpSocket::bind(("127.0.0.1", 18081))?;
        let dns = UdpSocket::bind(("127.0.0.1", 18082))?;
        tcp.set_nonblocking(true)?;
        udp.set_read_timeout(Some(Duration::from_millis(50)))?;
        dns.set_read_timeout(Some(Duration::from_millis(50)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        let done = stop.clone();
        threads.push(thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match tcp.accept() {
                    Ok((mut s, _)) => {
                        let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
                        let mut b = [0; 8192];
                        while !done.load(Ordering::Relaxed) {
                            match s.read(&mut b) {
                                Ok(0) => break,
                                Ok(n) => {
                                    if s.write_all(&b[..n]).is_err() {
                                        break;
                                    }
                                }
                                Err(e)
                                    if matches!(
                                        e.kind(),
                                        std::io::ErrorKind::WouldBlock
                                            | std::io::ErrorKind::TimedOut
                                    ) => {}
                                Err(_) => break,
                            }
                        }
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        }));
        for (socket, is_dns) in [(udp, false), (dns, true)] {
            let done = stop.clone();
            threads.push(thread::spawn(move || {
                let mut b = [0; 2048];
                while !done.load(Ordering::Relaxed) {
                    if let Ok((n, peer)) = socket.recv_from(&mut b) {
                        if is_dns {
                            if n < 12 {
                                continue;
                            }
                            b[2] = 0x81;
                            b[3] = 0x80;
                            b[6] = 0;
                            b[7] = 1;
                            let answer = [0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 10, 0, 2, 2];
                            b[n..n + 16].copy_from_slice(&answer);
                            let _ = socket.send_to(&b[..n + 16], peer);
                        } else {
                            let _ = socket.send_to(&b[..n], peer);
                        }
                    }
                }
            }));
        }
        Ok(Self {
            stop,
            threads,
            port: 18080,
        })
    }
}
impl Drop for Peers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}
pub fn verify(root: &Path) -> Result<()> {
    let peers = Peers::start()?;
    let iso = image(root, Mode::Normal)?;
    let vars = Temporary::new(root, "network")?;
    let logs = directory(root, "logs")?;
    let mut cmd = qemu(root, &iso, &vars.0, false)?;
    attach(&mut cmd);
    cmd.args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(logs.join("network.qemu.log"))?);
    let mut guest = Guest(cmd.spawn()?);
    let mut transcript = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing serial stdout")?,
        File::create(logs.join("network.serial.log"))?,
    )?;
    let mut input = guest.0.stdin.take().ok_or("Missing serial stdin")?;
    let start = Instant::now();
    let mut step = 0;
    let mut cursor = 0;
    while start.elapsed() < Duration::from_secs(90) {
        transcript.poll()?;
        if transcript.find_from(0, "[M10:FAIL]").is_some()
            || transcript.find_from(0, "[PANIC]").is_some()
        {
            return Err(format!("Network probe failed: {}", transcript.text()).into());
        }
        let expected = match step {
            0 => "anova> ",
            1 => "[M10:PASS] network=true refusal=true",
            _ => "[POWER:OFF]",
        };
        if let Some(at) = transcript.find_from(cursor, expected) {
            cursor = at + expected.len();
            match step {
                0 => {
                    writeln!(input, "network-test {}", peers.port)?;
                }
                1 => {
                    writeln!(input, "shutdown")?;
                }
                _ => {}
            }
            input.flush()?;
            step += 1;
        }
        if let Some(status) = guest.0.try_wait()? {
            transcript.finish()?;
            if step >= 2 && status.success() {
                println!(
                    "Verified controlled TCP/UDP/DNS, poll, timeouts, signals and socket reclamation"
                );
                return Ok(());
            }
            return Err(format!("Network guest exited {status}: {}", transcript.text()).into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(format!("Network watchdog expired: {}", transcript.text()).into())
}
