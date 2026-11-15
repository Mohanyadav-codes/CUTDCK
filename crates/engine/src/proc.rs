//! Running external tools (ffmpeg, yt-dlp, whisper) without console windows,
//! with cancellation and readable errors.

use anyhow::{Result, anyhow, bail};
use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// A command that never opens a console window and dies with its handle.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut c = Command::new(program);
    c.kill_on_drop(true).stdin(Stdio::null());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
}

fn tail(s: &str, lines: usize) -> String {
    let v: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    v[v.len().saturating_sub(lines)..].join("\n")
}

fn name_of(cmd: &Command) -> String {
    Path::new(cmd.as_std().get_program())
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "tool".into())
}

/// Run to completion, collecting output. Fails with the stderr tail on non-zero exit.
pub async fn run(mut cmd: Command, cancel: &CancellationToken) -> Result<Output> {
    let name = name_of(&cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| anyhow!("could not start {name}: {e}"))?;
    let out = tokio::select! {
        r = child.wait_with_output() => r?,
        _ = cancel.cancelled() => bail!("cancelled"),
    };
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        bail!("{name} failed ({}):\n{}", out.status, tail(&stderr, 12));
    }
    Ok(Output { stdout: out.stdout, stderr })
}

async fn pump<R: AsyncRead + Unpin>(r: R, tx: tokio::sync::mpsc::UnboundedSender<(bool, String)>, is_err: bool) {
    let mut lines = BufReader::new(r).lines();
    while let Ok(Some(l)) = lines.next_line().await {
        if tx.send((is_err, l)).is_err() {
            break;
        }
    }
}

/// Run and feed every stdout/stderr line to `on_line(is_stderr, line)`.
pub async fn run_lines(
    mut cmd: Command,
    cancel: &CancellationToken,
    mut on_line: impl FnMut(bool, &str),
) -> Result<()> {
    let name = name_of(&cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| anyhow!("could not start {name}: {e}"))?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let so = child.stdout.take().unwrap();
    let se = child.stderr.take().unwrap();
    tokio::spawn(pump(so, tx.clone(), false));
    tokio::spawn(pump(se, tx, true));
    let mut err_tail: Vec<String> = Vec::new();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some((is_err, line)) => {
                    if is_err {
                        err_tail.push(line.clone());
                        if err_tail.len() > 40 { err_tail.remove(0); }
                    }
                    on_line(is_err, &line);
                }
                None => break,
            },
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                bail!("cancelled");
            }
        }
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("{name} failed ({status}):\n{}", tail(&err_tail.join("\n"), 12));
    }
    Ok(())
}
