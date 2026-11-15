//! FFmpeg / FFprobe helpers.

use crate::deps::Tools;
use crate::proc::{command, run, run_lines};
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
}

#[derive(Deserialize)]
struct ProbeOut {
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}
#[derive(Deserialize)]
struct ProbeStream {
    codec_type: String,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    #[serde(default)]
    side_data_list: Vec<serde_json::Value>,
    #[serde(default)]
    tags: serde_json::Value,
}
#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

fn parse_rate(r: &str) -> Option<f64> {
    let (a, b) = r.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    if b == 0.0 || a == 0.0 { None } else { Some(a / b) }
}

pub async fn probe(tools: &Tools, path: &Path, cancel: &CancellationToken) -> Result<MediaInfo> {
    let mut c = command(&tools.ffprobe);
    c.args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"]).arg(path);
    let out = run(c, cancel).await?;
    let p: ProbeOut = serde_json::from_slice(&out.stdout).context("could not read the video's details")?;
    let v = p.streams.iter().find(|s| s.codec_type == "video").ok_or_else(|| anyhow!("this file has no video track"))?;
    let mut w = v.width.unwrap_or(0);
    let mut h = v.height.unwrap_or(0);
    // Rotated phone videos report landscape dimensions plus a rotation.
    let rot = v
        .side_data_list
        .iter()
        .filter_map(|s| s.get("rotation").and_then(|r| r.as_f64()))
        .next()
        .or_else(|| v.tags.get("rotate").and_then(|r| r.as_str()).and_then(|r| r.parse().ok()))
        .unwrap_or(0.0);
    if (rot.abs() - 90.0).abs() < 1.0 || (rot.abs() - 270.0).abs() < 1.0 {
        std::mem::swap(&mut w, &mut h);
    }
    let fps = v
        .avg_frame_rate
        .as_deref()
        .and_then(parse_rate)
        .or_else(|| v.r_frame_rate.as_deref().and_then(parse_rate))
        .unwrap_or(30.0);
    let duration = p.format.duration.and_then(|d| d.parse().ok()).unwrap_or(0.0);
    Ok(MediaInfo { duration, width: w, height: h, fps, has_audio: p.streams.iter().any(|s| s.codec_type == "audio") })
}

/// FFmpeg with `-progress` reporting seconds processed.
pub async fn ffmpeg_progress(
    tools: &Tools,
    args: Vec<String>,
    cwd: Option<&Path>,
    total: f64,
    cancel: &CancellationToken,
    mut on: impl FnMut(f64),
) -> Result<()> {
    let mut c = command(&tools.ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
    c.args(&args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    run_lines(c, cancel, |is_err, line| {
        if !is_err {
            if let Some(v) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
                if let Ok(us) = v.trim().parse::<f64>() {
                    if total > 0.0 {
                        on((us / 1_000_000.0 / total).clamp(0.0, 1.0));
                    }
                }
            }
        }
    })
    .await
}

/// 16 kHz mono 16-bit WAV for analysis and on-device transcription.
pub async fn extract_wav(tools: &Tools, input: &Path, out: &Path, total: f64, cancel: &CancellationToken, on: impl FnMut(f64)) -> Result<()> {
    let tmp = out.with_extension("tmp.wav");
    let args = vec![
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        tmp.to_string_lossy().to_string(),
    ];
    ffmpeg_progress(tools, args, None, total, cancel, on).await?;
    std::fs::rename(&tmp, out)?;
    Ok(())
}

/// Encode a slice of the WAV as Opus (small upload for cloud transcription).
pub async fn encode_opus_slice(tools: &Tools, wav: &Path, start: f64, dur: f64, out: &Path, cancel: &CancellationToken) -> Result<()> {
    let mut c = command(&tools.ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
    c.args(["-ss", &format!("{start:.3}"), "-t", &format!("{dur:.3}"), "-i"]).arg(wav);
    c.args(["-ac", "1", "-c:a", "libopus", "-b:a", "32k", "-application", "voip"]).arg(out);
    run(c, cancel).await?;
    Ok(())
}

/// Read 16-bit mono PCM samples from our WAV (skips the RIFF header).
pub fn read_wav_samples(wav: &Path) -> Result<Vec<i16>> {
    let bytes = std::fs::read(wav)?;
    // Find the "data" chunk.
    let mut i = 12usize;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
        if id == b"data" {
            let start = i + 8;
            let end = (start + len).min(bytes.len());
            let data = &bytes[start..end];
            return Ok(data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect());
        }
        i += 8 + len + (len & 1);
    }
    bail!("unexpected audio format")
}

/// Loudness in dBFS per `hop` seconds window.
pub fn loudness_curve(samples: &[i16], sr: u32, hop: f64) -> Vec<f32> {
    let n = ((sr as f64) * hop) as usize;
    samples
        .chunks(n.max(1))
        .map(|c| {
            let sum: f64 = c.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
            let rms = (sum / c.len().max(1) as f64).sqrt();
            (20.0 * rms.max(1e-6).log10()) as f32
        })
        .collect()
}

/// Cut a section into an edit-friendly intermediate (constant frame rate, fast seek).
pub async fn extract_section(
    tools: &Tools,
    input: &Path,
    start: f64,
    dur: f64,
    fps: f64,
    has_audio: bool,
    out: &Path,
    cancel: &CancellationToken,
    on: impl FnMut(f64),
) -> Result<()> {
    let tmp = out.with_extension("tmp.mp4");
    let mut args: Vec<String> = vec![
        "-ss".into(),
        format!("{start:.3}"),
        "-i".into(),
        input.to_string_lossy().to_string(),
    ];
    if !has_audio {
        args.extend(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"].map(String::from));
    }
    args.extend(vec![
        "-t".into(),
        format!("{dur:.3}"),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        if has_audio { "0:a:0".into() } else { "1:a:0".into() },
        "-vf".into(),
        format!("fps={fps},scale='min(1920,iw)':-2"),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "16".into(),
        "-g".into(),
        "15".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-ar".into(),
        "48000".into(),
        tmp.to_string_lossy().to_string(),
    ]);
    ffmpeg_progress(tools, args, None, dur, cancel, on).await?;
    std::fs::rename(&tmp, out)?;
    Ok(())
}

/// Decode grayscale frames at `fps`, scaled to `width`, calling `on_frame(t, w, h, pixels)`.
pub async fn gray_frames(
    tools: &Tools,
    input: &Path,
    fps: f64,
    width: u32,
    src_w: u32,
    src_h: u32,
    cancel: &CancellationToken,
    mut on_frame: impl FnMut(f64, u32, u32, &[u8]),
) -> Result<()> {
    let w = width.min(src_w.max(2)) & !1;
    let h = (((src_h as f64) * (w as f64) / (src_w.max(1) as f64)).round() as u32).max(2) & !1;
    let mut c = command(&tools.ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"]).arg(input);
    c.args(["-vf", &format!("fps={fps},scale={w}:{h}"), "-pix_fmt", "gray", "-f", "rawvideo", "-"]);
    c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    let mut child = c.spawn().context("could not start ffmpeg")?;
    let mut so = child.stdout.take().unwrap();
    let size = (w * h) as usize;
    let mut buf = vec![0u8; size];
    let mut i = 0u64;
    loop {
        let read = tokio::select! {
            r = so.read_exact(&mut buf) => r,
            _ = cancel.cancelled() => { let _ = child.kill().await; bail!("cancelled"); }
        };
        match read {
            Ok(_) => {
                on_frame(i as f64 / fps, w, h, &buf);
                i += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
    }
    let _ = child.wait().await;
    Ok(())
}

/// Grab a single JPEG frame.
pub async fn thumbnail(tools: &Tools, input: &Path, at: f64, out: &Path, cancel: &CancellationToken) -> Result<()> {
    let mut c = command(&tools.ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-ss", &format!("{at:.2}"), "-i"]).arg(input);
    c.args(["-frames:v", "1", "-q:v", "3"]).arg(out);
    run(c, cancel).await?;
    Ok(())
}

/// Find a working H.264 hardware encoder, or fall back to libx264.
pub async fn detect_encoder(tools: &Tools) -> String {
    let cands: &[&str] = if cfg!(target_os = "macos") {
        &["h264_videotoolbox"]
    } else {
        &["h264_nvenc", "h264_qsv", "h264_amf"]
    };
    for enc in cands {
        let mut c = command(&tools.ffmpeg);
        c.args([
            "-hide_banner", "-nostdin", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=black:s=1080x1920:d=0.2", "-frames:v", "3", "-c:v", enc, "-f",
            "null", "-",
        ]);
        if run(c, &CancellationToken::new()).await.is_ok() {
            return enc.to_string();
        }
    }
    "libx264".into()
}

/// Encoder arguments for the final export.
pub fn encoder_args(enc: &str) -> Vec<String> {
    let v: Vec<&str> = match enc {
        "h264_nvenc" => vec!["-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "21", "-b:v", "0"],
        "h264_qsv" => vec!["-c:v", "h264_qsv", "-preset", "medium", "-global_quality", "22"],
        "h264_amf" => vec!["-c:v", "h264_amf", "-quality", "balanced", "-rc", "cqp", "-qp_i", "21", "-qp_p", "23"],
        "h264_videotoolbox" => vec!["-c:v", "h264_videotoolbox", "-b:v", "9M"],
        _ => vec!["-c:v", "libx264", "-preset", "veryfast", "-crf", "20"],
    };
    v.into_iter().map(String::from).collect()
}
