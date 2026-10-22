// SPDX-License-Identifier: Apache-2.0
//! Drive native/Linux CLI through COM1, checking dynamic musl, Ctrl+C, editing, EOF and volatile tmpfs.
//! Watchdogs kill only these disposable QEMU guests; a prompt alone is never success evidence.
use super::*;
use std::io::Write;

pub fn verify(root: &Path) -> Result<()> {
    let iso = image(root, Mode::Normal)?;
    session(root, &iso, false)?;
    session(root, &iso, true)?;
    println!("Verified serial editing/CRLF/EOF, redirection and fresh-boot tmpfs loss");
    Ok(())
}
fn session(root: &Path, iso: &Path, reboot: bool) -> Result<()> {
    let name = if reboot {
        "terminal-reboot"
    } else {
        "terminal"
    };
    let logs = directory(root, "logs")?;
    let serial = logs.join(format!("{name}.serial.log"));
    let diagnostic = logs.join(format!("{name}.qemu.log"));
    output_path(&serial)?;
    output_path(&diagnostic)?;
    let output = File::create(&serial)?;
    let error = File::create(&diagnostic)?;
    let vars = Temporary::new(root, "terminal")?;
    let mut cmd = qemu(root, iso, &vars.0, false)?;
    cmd.args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(error);
    let mut guest = Guest(cmd.spawn()?);
    let mut transcript = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing UART output pipe")?,
        output,
    )?;
    let mut input = guest.0.stdin.take().ok_or("Missing UART input pipe")?;
    let exchanges: &[(&str, &[u8])] = if reboot {
        &[
            ("anova> ", b"cat /tmp/terminal\r"),
            ("cat: failed (-7)", b"status\r"),
            ("[SHELL:STATUS] code=1", b"\x04"),
        ]
    } else {
        &[
            ("anova> ", b"echo tty-editX\x7f\r\n"),
            ("\r\ntty-edit\r\n", b"echo terminal-file > /tmp/terminal\r"),
            (
                "anova> echo terminal-file > /tmp/terminal",
                b"cat /tmp/terminal\r",
            ),
            ("\r\nterminal-file\r\n", b"linux-musl test\r"),
            ("[LINUX:MUSL] TLS=true malloc=true errno=true file=true", b"linux-dynamic\r"),
            ("[M9:DYNAMIC] interpreter=true shared_library=true dlopen=true dynamic_TLS=true missing_library=true",b"linux-threads interrupt\r"),
            ("[M9:UART] waiting",b"\x03"),
            ("[M9:UART] CtrlC=true EINTR=true",b"/linux/sh\r"),
            ("[LINUX:EXEC] image=/linux/sh dynamic=false", b"/linux/echo M8-UART | /linux/cat > /tmp/linux-uart; /linux/cat /tmp/linux-uart; exit\r"),
            ("\r\nM8-UART\r\n", b"status\r"),
            ("[SHELL:STATUS] code=0", b"\x04"),
        ]
    };
    let start = Instant::now();
    let mut step = 0;
    let mut ready_at = None;
    let mut observed = 0;
    loop {
        if let Some(exit) = guest.0.try_wait()? {
            return Err(format!(
                "{name}: guest exited early {exit}; see {}",
                serial.display()
            )
            .into());
        }
        transcript.poll()?;
        let text = transcript.text();
        if text.contains("[PANIC]")
            || text.contains("[EXCEPTION]")
            || text.contains("[INIT:FAIL]")
            || text.contains("[SHELL:FAIL]")
        {
            return Err(format!("{name}: guest failure; see {}", serial.display()).into());
        }
        if step < exchanges.len()
            && let Some(at) = transcript.find_from(observed, exchanges[step].0)
        {
            // Let the shell actually enter its next blocking read before sending more input.
            let since = *ready_at.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(80) {
                observed = at + exchanges[step].0.len();
                input.write_all(exchanges[step].1)?;
                input.flush()?;
                step += 1;
                ready_at = None;
            }
        }
        if step == exchanges.len()
            && text.contains("[INIT] shell exited; starting a new session")
            && text.matches("AnovaOS native shell.").count() >= 2
        {
            if !reboot && (!text.contains("\x08 \x08") || text.contains("shell: syntax")) {
                return Err("Terminal editing or CRLF normalization failed".into());
            }
            break;
        }
        if start.elapsed() > Duration::from_secs(30) {
            return Err(format!(
                "{name}: UART scenario timed out at step {step}; see {}",
                serial.display()
            )
            .into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    guest.0.kill()?;
    guest.0.wait()?;
    transcript.finish()?;
    drop(guest);
    vars.cleanup()?;
    let evidence = logs.join(format!("{name}.result.txt"));
    output_path(&evidence)?;
    fs::write(
        evidence,
        format!(
            "scenario={name}\nresult=Ok\nsteps={step}\nnormal_guest_killed_after_verified_shell_restart=true\n"
        ),
    )?;
    Ok(())
}
