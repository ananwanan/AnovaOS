// SPDX-License-Identifier: Apache-2.0
//! Capture ordered guest stdout directly while retaining an on-disk UART log for diagnostics.
//! Match raw bytes and complete status lines so fragmented UART data cannot invalidate results.
use std::{
    borrow::Cow,
    io::{self, Read, Write},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

const LIMIT: usize = 8 * 1024 * 1024;

pub struct Transcript {
    receiver: Receiver<io::Result<Vec<u8>>>,
    bytes: Vec<u8>,
    closed: bool,
}

impl Transcript {
    pub fn capture(
        mut input: impl Read + Send + 'static,
        mut log: impl Write + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(16);
        thread::Builder::new()
            .name("guest-serial".into())
            .spawn(move || {
                let result = (|| -> io::Result<()> {
                    let mut buffer = [0; 4096];
                    loop {
                        let length = match input.read(&mut buffer) {
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                            result => result?,
                        };
                        if length == 0 {
                            return log.flush();
                        }
                        log.write_all(&buffer[..length])?;
                        if sender.send(Ok(buffer[..length].to_vec())).is_err() {
                            return Ok(());
                        }
                    }
                })();
                if let Err(error) = result {
                    let _ = sender.send(Err(error));
                }
            })?;
        Ok(Self {
            receiver,
            bytes: Vec::new(),
            closed: false,
        })
    }

    fn append(&mut self, chunk: io::Result<Vec<u8>>) -> io::Result<()> {
        let chunk = chunk?;
        if chunk.len() > LIMIT - self.bytes.len() {
            return Err(io::Error::other("Guest serial transcript exceeded 8 MiB"));
        }
        self.bytes.extend_from_slice(&chunk);
        Ok(())
    }

    pub fn poll(&mut self) -> io::Result<()> {
        loop {
            match self.receiver.try_recv() {
                Ok(chunk) => self.append(chunk)?,
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    return Ok(());
                }
            }
        }
    }

    /// Call only after the guest exits or is killed; its stdout must close promptly.
    pub fn finish(&mut self) -> io::Result<()> {
        while !self.closed {
            match self.receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(chunk) => self.append(chunk)?,
                Err(mpsc::RecvTimeoutError::Disconnected) => self.closed = true,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(io::Error::other(
                        "Guest serial pipe did not close after exit",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }

    /// Mark the exact byte boundary before a new input, independently of lossy UTF-8 decoding.
    pub fn position(&self) -> usize {
        self.bytes.len()
    }

    /// Return the absolute byte position, never an index into lossy UTF-8 text.
    pub fn find_from(&self, start: usize, marker: &str) -> Option<usize> {
        let tail = self.bytes.get(start..)?;
        if marker.is_empty() {
            return Some(start);
        }
        tail.windows(marker.len())
            .position(|window| window == marker.as_bytes())
            .map(|offset| start + offset)
    }

    /// Status output may split at any byte, including between '=' and the exit code.
    /// Consume only a complete line so a partial success cannot be classified as failure.
    pub fn shell_status_from(&self, start: usize) -> Option<u32> {
        for line in self
            .bytes
            .get(start..)?
            .split_inclusive(|byte| *byte == b'\n')
        {
            let Some(line) = line.strip_suffix(b"\n") else {
                continue;
            };
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let marker = b"[SHELL:STATUS] code=";
            if let Some(at) = line.windows(marker.len()).position(|bytes| bytes == marker) {
                return std::str::from_utf8(&line[at + marker.len()..])
                    .ok()?
                    .parse()
                    .ok();
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_status_waits_for_the_complete_fragmented_line() {
        let (sender, receiver) = mpsc::sync_channel(4);
        let mut transcript = Transcript {
            receiver,
            bytes: Vec::new(),
            closed: false,
        };
        sender.send(Ok(b"[SHELL:STATUS] code=".to_vec())).unwrap();
        transcript.poll().unwrap();
        assert_eq!(transcript.shell_status_from(0), None);
        sender.send(Ok(b"0\r".to_vec())).unwrap();
        transcript.poll().unwrap();
        assert_eq!(transcript.shell_status_from(0), None);
        sender
            .send(Ok(b"\nanova> [SHELL:STATUS] code=101\r\n".to_vec()))
            .unwrap();
        transcript.poll().unwrap();
        assert_eq!(transcript.shell_status_from(0), Some(0));
        assert_eq!(transcript.shell_status_from(23), Some(101));
        assert_eq!(transcript.shell_status_from(1000), None);
    }

    #[test]
    fn split_markers_and_incomplete_utf8_keep_raw_cursors_stable() {
        let (sender, receiver) = mpsc::sync_channel(4);
        let mut transcript = Transcript {
            receiver,
            bytes: Vec::new(),
            closed: false,
        };
        sender.send(Ok(b"ready\n\xe7".to_vec())).unwrap();
        transcript.poll().unwrap();
        assert_eq!(transcript.find_from(0, "ready"), Some(0));
        assert_eq!(transcript.find_from(100, "ready"), None);
        sender.send(Ok(b"\x8c\xab[M9:UA".to_vec())).unwrap();
        transcript.poll().unwrap();
        assert_eq!(transcript.find_from(5, "[M9:UART]"), None);
        sender.send(Ok(b"RT] waiting".to_vec())).unwrap();
        drop(sender);
        transcript.finish().unwrap();
        assert_eq!(transcript.text(), "ready\n猫[M9:UART] waiting");
        assert_eq!(transcript.find_from(5, "[M9:UART]"), Some(9));
        assert_eq!(transcript.find_from(10, "[M9:UART]"), None);
    }

    #[test]
    fn reader_drains_final_output_and_propagates_log_write_errors() {
        let mut transcript =
            Transcript::capture(io::Cursor::new(b"final output"), io::sink()).unwrap();
        transcript.finish().unwrap();
        assert_eq!(transcript.text(), "final output");

        struct FailedLog;
        impl Write for FailedLog {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("Diagnostic disk full"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut transcript = Transcript::capture(io::Cursor::new(b"output"), FailedLog).unwrap();
        assert_eq!(
            transcript.finish().unwrap_err().to_string(),
            "Diagnostic disk full"
        );
    }
}
