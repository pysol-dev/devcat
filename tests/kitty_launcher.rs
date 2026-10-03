//! The real launcher driven by a simulated Kitty terminal (not visual certification).
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[expect(
    clippy::too_many_lines,
    reason = "Single real PTY scenario includes its teardown"
)]
#[test]
fn kitty_negotiates_and_preserves_input_when_terminal_responds() -> Result<()> {
    // Given a real outer PTY and a synthetic terminal protocol responder.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let evidence = std::env::var_os("BONGO_EVIDENCE_DIR")
        .map_or_else(|| root.join(".omo/evidence/kitty-launcher"), PathBuf::from);
    std::fs::create_dir_all(&evidence)?;
    for (supported, respond_allocation) in [(true, true), (false, false), (true, false)] {
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_bongo"));
        cmd.args(["--renderer", "auto", "--"]);
        cmd.arg(root.join("target/debug/examples/composer-fixture"));
        cmd.env("BONGO_TEST_PROFILE", "fixture");
        cmd.env_remove("TMUX");
        cmd.env_remove("ZELLIJ");
        cmd.env_remove("STY");
        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let (send, receive) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 || send.send(buffer[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        // When query/allocation replies and seven composing keys reach the launcher.
        let mut raw = Vec::new();
        let mut probe = false;
        let mut allocation = false;
        let mut typed = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let Ok(bytes) = receive.recv_timeout(Duration::from_millis(300)) else {
                if typed && child.try_wait()?.is_some() {
                    break;
                }
                continue;
            };
            raw.extend(bytes);
            let text = String::from_utf8_lossy(&raw);
            if !probe && text.contains("a=q") {
                probe = true;
                if supported {
                    writer.write_all(b"pre\x1b[12;34R\x1b_Gi=31;OK\x1b\\post")?;
                } else {
                    writer.write_all(b"pre\x1b[12;34Rpost")?;
                }
            }
            if supported
                && !allocation
                && let Some(start) = text.find("a=t,t=d,f=100,I=")
            {
                let number = text[start + 16..].split(',').next().unwrap_or("");
                if number.parse::<u32>().is_ok() {
                    if respond_allocation {
                        write!(writer, "\x1b_Gi=90210,I={number};OK\x1b\\")?;
                    }
                    allocation = true;
                }
            }
            if !typed && text.contains("controlled composer fixture") {
                for key in b"abcdef\x7f" {
                    writer.write_all(&[*key])?;
                    std::thread::sleep(Duration::from_millis(70));
                }
                writer.write_all(b"\x04")?;
                typed = true;
            }
        }
        let exited = child.try_wait()?.is_some();
        if !exited {
            child.kill()?;
        }
        let status = child.wait()?;
        drop(writer);
        drop(pair.master);
        thread.join().map_err(|_| anyhow::anyhow!("reader panic"))?;
        while let Ok(bytes) = receive.try_recv() {
            raw.extend(bytes);
        }
        let name = match (supported, respond_allocation) {
            (true, true) => "supported",
            (true, false) => "allocation-timeout",
            (false, _) => "fallback",
        };
        std::fs::write(evidence.join(format!("{name}.bin")), &raw)?;
        std::fs::write(
            evidence.join(format!("{name}-cleanup.txt")),
            format!(
                "exited={exited}\nstatus={}\nreader joined; PTY handles dropped\n",
                status.exit_code()
            ),
        )?;
        ensure!(
            probe && typed && exited && status.exit_code() == 37,
            "launcher scenario {name} failed"
        );
        let text = String::from_utf8_lossy(&raw);
        let receipt = text
            .split("FIXTURE_RECEIPT:")
            .nth(1)
            .and_then(|s| s.lines().next())
            .ok_or_else(|| anyhow::anyhow!("missing receipt"))?;
        let receipt: serde_json::Value = serde_json::from_str(receipt.trim())?;
        ensure!(
            receipt["bytes"] == serde_json::json!(b"pre\x1b[12;34Rpostabcdef\x7f\x04".to_vec()),
            "input was not preserved: {}",
            receipt["bytes"]
        );
        if supported && respond_allocation {
            ensure!(
                allocation
                    && text.contains("a=p,i=90210,p=1,C=1,z=-1,c=16,r=4")
                    && text.contains("a=d,d=I,i=90210"),
                "missing graphics placement/cleanup"
            );
            let mut pngs = Vec::new();
            let mut payload = String::new();
            for apc in text.split("\x1b_G").skip(1) {
                let Some((command, _)) = apc.split_once("\x1b\\") else {
                    continue;
                };
                let Some((header, data)) = command.split_once(';') else {
                    continue;
                };
                if header.contains("a=t,") {
                    payload.clear();
                }
                if header.contains("m=") {
                    payload.push_str(data);
                    if header.contains("m=0") {
                        pngs.push(STANDARD.decode(&payload)?);
                    }
                }
            }
            for pose in ["idle", "left", "right"] {
                ensure!(
                    pngs.contains(&std::fs::read(
                        root.join(format!("assets/generated/{pose}.png"))
                    )?),
                    "missing original {pose} PNG"
                );
            }
        } else {
            ensure!(
                !text.contains("a=p,"),
                "unsupported terminal received placement"
            );
        }
    }
    Ok(())
}
