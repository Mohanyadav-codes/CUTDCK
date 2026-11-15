//! Links (YouTube, Kick, Twitch VODs and anything yt-dlp supports).

use crate::deps::Tools;
use crate::proc::{command, run, run_lines};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub fn is_url(s: &str) -> bool {
    let t = s.trim().to_lowercase();
    t.starts_with("http://") || t.starts_with("https://")
}

/// Strip tracking junk so the same video maps to the same project.
pub fn normalize_url(s: &str) -> String {
    let t = s.trim();
    let re = regex::Regex::new(r"(?:youtube\.com/(?:watch\?(?:.*&)?v=|shorts/|live/)|youtu\.be/)([A-Za-z0-9_-]{11})").unwrap();
    if let Some(c) = re.captures(t) {
        return format!("https://www.youtube.com/watch?v={}", &c[1]);
    }
    t.to_string()
}

#[derive(Debug, Deserialize)]
pub struct LinkInfo {
    pub id: Option<String>,
    pub title: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub thumbnail: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub live_status: Option<String>,
    pub is_live: Option<bool>,
    #[serde(default)]
    pub subtitles: serde_json::Value,
}

fn ytdlp(tools: &Tools) -> Result<tokio::process::Command> {
    let y = tools.ytdlp.as_ref().ok_or_else(|| anyhow!("yt-dlp is not installed yet"))?;
    let mut c = command(y);
    if let Some(d) = &tools.deno_dir {
        // yt-dlp finds Deno on PATH; it needs it to solve YouTube's JS challenges.
        let mut paths = vec![d.clone()];
        if let Some(p) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            c.env("PATH", joined);
        }
    }
    if let Some(dir) = tools.ffmpeg.parent() {
        c.arg("--ffmpeg-location").arg(dir);
    }
    c.args(["--no-playlist", "--no-warnings"]);
    Ok(c)
}

pub async fn info(tools: &Tools, url: &str, cancel: &CancellationToken) -> Result<LinkInfo> {
    let mut c = ytdlp(tools)?;
    c.args(["-J", "--skip-download", url]);
    let out = run(c, cancel).await.map_err(|e| anyhow!(friendly(&e.to_string())))?;
    let info: LinkInfo = serde_json::from_slice(&out.stdout).context("could not read the link's details")?;
    if info.is_live == Some(true) || info.live_status.as_deref() == Some("is_live") {
        return Err(anyhow!("This stream is still live. Cutdeck can clip it once the VOD is available."));
    }
    Ok(info)
}

fn friendly(e: &str) -> String {
    let l = e.to_lowercase();
    if l.contains("private video") {
        "This video is private.".into()
    } else if l.contains("sign in to confirm") || l.contains("not a bot") {
        "YouTube asked for a sign-in check. Try again in a few minutes.".into()
    } else if l.contains("unsupported url") {
        "Cutdeck can't read this link. Try a YouTube, Kick or Twitch video link.".into()
    } else if l.contains("video unavailable") {
        "This video is unavailable.".into()
    } else {
        e.to_string()
    }
}

fn parse_percent(line: &str) -> Option<f64> {
    let p = line.strip_prefix("cdprog:")?.trim().trim_end_matches('%').trim();
    p.parse::<f64>().ok().map(|v| v / 100.0)
}

/// Download the audio only (small and fast) for analysis.
pub async fn download_audio(tools: &Tools, url: &str, dir: &Path, cancel: &CancellationToken, mut on: impl FnMut(f64)) -> Result<PathBuf> {
    let mut c = ytdlp(tools)?;
    c.args(["-f", "ba[ext=m4a]/ba/b", "--newline", "--progress-template", "download:cdprog:%(progress._percent_str)s"]);
    c.arg("-o").arg(dir.join("source_audio.%(ext)s"));
    c.args(["--print", "after_move:filepath", url]);
    let mut path = None;
    run_lines(c, cancel, |is_err, line| {
        if let Some(p) = parse_percent(line) {
            on(p);
        } else if !is_err && !line.trim().is_empty() && Path::new(line.trim()).exists() {
            path = Some(PathBuf::from(line.trim()));
        }
    })
    .await
    .map_err(|e| anyhow!(friendly(&e.to_string())))?;
    path.or_else(|| find_prefixed(dir, "source_audio.")).ok_or_else(|| anyhow!("audio download produced no file"))
}

fn find_prefixed(dir: &Path, prefix: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().map(|n| n.to_string_lossy().starts_with(prefix)).unwrap_or(false) && !p.to_string_lossy().ends_with(".part")
    })
}

/// Download only `start..end` of the video at up to 1080p.
pub async fn download_section(
    tools: &Tools,
    url: &str,
    start: f64,
    end: f64,
    out_stem: &Path,
    cancel: &CancellationToken,
    mut on: impl FnMut(f64),
) -> Result<PathBuf> {
    let mut c = ytdlp(tools)?;
    c.args(["-f", "bv*[height<=1080][vcodec!*=av01]+ba/bv*[height<=1080]+ba/b[height<=1080]/b"]);
    c.args(["--download-sections", &format!("*{start:.2}-{end:.2}"), "--force-keyframes-at-cuts"]);
    c.args(["--merge-output-format", "mp4", "--newline", "--progress-template", "download:cdprog:%(progress._percent_str)s"]);
    let stem = out_stem.to_string_lossy().to_string();
    c.arg("-o").arg(format!("{stem}.%(ext)s"));
    c.args(["--print", "after_move:filepath", url]);
    let mut path = None;
    run_lines(c, cancel, |is_err, line| {
        if let Some(p) = parse_percent(line) {
            on(p);
        } else if !is_err && Path::new(line.trim()).is_file() {
            path = Some(PathBuf::from(line.trim()));
        }
    })
    .await
    .map_err(|e| anyhow!(friendly(&e.to_string())))?;
    let name = out_stem.file_name().unwrap().to_string_lossy().to_string();
    path.or_else(|| find_prefixed(out_stem.parent().unwrap(), &format!("{name}.")))
        .ok_or_else(|| anyhow!("video download produced no file"))
}

/// Live-chat replay for past YouTube streams: message timestamps in seconds.
/// Best effort with a time limit; returns what was fetched.
pub async fn chat_times(tools: &Tools, url: &str, dir: &Path, limit_secs: u64) -> Vec<f64> {
    let Ok(mut c) = ytdlp(tools) else { return vec![] };
    c.args(["--skip-download", "--write-subs", "--sub-langs", "live_chat"]);
    c.arg("-o").arg(dir.join("chat.%(ext)s"));
    c.arg(url);
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(limit_secs)).await;
        c2.cancel();
    });
    let _ = run(c, &cancel).await;
    let file = std::fs::read_dir(dir)
        .ok()
        .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.to_string_lossy().contains("live_chat.json")));
    let Some(f) = file else { return vec![] };
    let Ok(text) = std::fs::read_to_string(f) else { return vec![] };
    let re = regex::Regex::new(r#""videoOffsetTimeMsec"\s*:\s*"(\d+)""#).unwrap();
    text.lines().filter_map(|l| re.captures(l).and_then(|c| c[1].parse::<f64>().ok()).map(|ms| ms / 1000.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls() {
        assert!(is_url("https://youtu.be/abc"));
        assert!(!is_url("C:\\video.mp4"));
        assert_eq!(normalize_url("https://youtu.be/dQw4w9WgXcQ?si=xx"), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?app=desktop&v=dQw4w9WgXcQ&t=20"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(normalize_url("https://kick.com/video/123"), "https://kick.com/video/123");
        assert_eq!(parse_percent("cdprog: 42.5%"), Some(0.425));
    }
}
