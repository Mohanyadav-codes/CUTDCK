//! Links (YouTube, Kick, Twitch VODs and anything yt-dlp supports).

use crate::deps::Tools;
use crate::proc::{command, run, run_lines};
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
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
    // Ignore any yt-dlp config on the user's machine so behaviour is the same for everyone.
    c.args(["--ignore-config", "--no-playlist", "--no-warnings", "--retries", "5", "--fragment-retries", "10", "--retry-sleep", "exp=1:20"]);
    Ok(c)
}

// ---------------------------------------------------------------------------
// Staying unblocked. YouTube changes often and sometimes asks for a sign-in
// check. Cutdeck keeps yt-dlp on its nightly channel (yt-dlp's recommended
// channel), and when a request is blocked it updates yt-dlp, waits, and then
// retries with the user's browser sign-in if they allow it.
// ---------------------------------------------------------------------------

static BROWSER_LOGIN: AtomicBool = AtomicBool::new(true);
/// Last time yt-dlp was updated in this session.
static UPDATED: OnceLock<tokio::sync::Mutex<Option<Instant>>> = OnceLock::new();
/// The browser whose cookies last got past a block, reused for later requests.
static GOOD_BROWSER: Mutex<Option<String>> = Mutex::new(None);

/// Allow or forbid falling back to the browser's YouTube sign-in (a setting).
pub fn set_browser_login(on: bool) {
    BROWSER_LOGIN.store(on, Ordering::Relaxed);
    if !on {
        *GOOD_BROWSER.lock().unwrap() = None;
    }
}

/// Update yt-dlp to the latest nightly. Skipped if it ran in the last six
/// hours, or the last ten minutes when `force` is set after a block.
/// Returns true when an update was attempted.
pub async fn keep_fresh(tools: &Tools, force: bool) -> bool {
    let Some(y) = tools.ytdlp.clone() else { return false };
    let lock = UPDATED.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut last = lock.lock().await;
    let gap = if force { Duration::from_secs(600) } else { Duration::from_secs(6 * 3600) };
    if last.is_some_and(|t| t.elapsed() < gap) {
        return false;
    }
    *last = Some(Instant::now());
    let mut c = command(&y);
    c.args(["--update-to", "nightly"]);
    let cancel = CancellationToken::new();
    let timer = {
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(90)).await;
            c2.cancel();
        })
    };
    let _ = run(c, &cancel).await;
    timer.abort();
    true
}

/// Errors that no retry can fix.
fn is_final(e: &str) -> bool {
    let l = e.to_lowercase();
    ["cancelled", "private video", "unsupported url", "is not a valid url", "has been removed", "account associated with this video has been terminated", "copyright", "members-only", "join this channel", "not installed yet"]
        .iter()
        .any(|k| l.contains(k))
}

/// The browser's cookies could not be read (browser open, encrypted, missing).
fn is_cookie_error(e: &str) -> bool {
    let l = e.to_lowercase();
    ["cookie", "dpapi", "decrypt", "keyring", "could not find"].iter().any(|k| l.contains(k))
}

/// Browsers installed for this user, best candidates first. Firefox comes
/// first because its cookies can be read while it is open.
fn browsers() -> Vec<&'static str> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut out = Vec::new();
    if cfg!(windows) {
        let (Some(roam), Some(local)) = (env("APPDATA"), env("LOCALAPPDATA")) else { return out };
        let list: [(&str, PathBuf); 6] = [
            ("firefox", roam.join("Mozilla").join("Firefox").join("Profiles")),
            ("brave", local.join("BraveSoftware").join("Brave-Browser").join("User Data")),
            ("edge", local.join("Microsoft").join("Edge").join("User Data")),
            ("chrome", local.join("Google").join("Chrome").join("User Data")),
            ("opera", roam.join("Opera Software").join("Opera Stable")),
            ("vivaldi", local.join("Vivaldi").join("User Data")),
        ];
        for (name, dir) in list {
            if dir.is_dir() {
                out.push(name);
            }
        }
    } else if let Some(home) = env("HOME") {
        let sup = home.join("Library").join("Application Support");
        let list: [(&str, PathBuf); 5] = [
            ("firefox", sup.join("Firefox").join("Profiles")),
            ("safari", home.join("Library").join("Cookies")),
            ("chrome", sup.join("Google").join("Chrome")),
            ("brave", sup.join("BraveSoftware").join("Brave-Browser")),
            ("edge", sup.join("Microsoft Edge")),
        ];
        for (name, dir) in list {
            if dir.is_dir() {
                out.push(name);
            }
        }
    }
    out
}

fn cookie_args(browser: &str) -> Vec<String> {
    vec!["--cookies-from-browser".into(), browser.into()]
}

/// Steps through fallbacks for one yt-dlp request when it is blocked:
/// 1. as is (with the browser that worked before, if any),
/// 2. after updating yt-dlp and a short wait,
/// 3. with each installed browser's YouTube sign-in, if allowed.
struct Fallbacks {
    plan: Vec<Option<String>>,
    next: usize,
    current: Option<String>,
    remembered: Option<String>,
    allow: bool,
    updated: bool,
    browsers_queued: bool,
    first_err: Option<String>,
}

impl Fallbacks {
    async fn start(tools: &Tools) -> Self {
        keep_fresh(tools, false).await;
        let allow = BROWSER_LOGIN.load(Ordering::Relaxed);
        let remembered = if allow { GOOD_BROWSER.lock().unwrap().clone() } else { None };
        Self { plan: vec![remembered.clone()], next: 0, current: None, remembered, allow, updated: false, browsers_queued: false, first_err: None }
    }

    /// Extra yt-dlp arguments for the next attempt, or None when out of options.
    fn next_args(&mut self) -> Option<Vec<String>> {
        let b = self.plan.get(self.next)?.clone();
        self.next += 1;
        let args = b.as_deref().map(cookie_args).unwrap_or_default();
        self.current = b;
        Some(args)
    }

    fn succeeded(&self) {
        if let Some(b) = &self.current {
            *GOOD_BROWSER.lock().unwrap() = Some(b.clone());
        }
    }

    /// Record a failure. Returns an error when retrying cannot help.
    async fn failed(&mut self, tools: &Tools, cancel: &CancellationToken, e: anyhow::Error) -> Result<()> {
        let err = format!("{e:#}");
        if cancel.is_cancelled() || is_final(&err) {
            bail!(friendly(&err));
        }
        let browser = self.current.clone();
        if browser.is_some() && browser == self.remembered {
            // The browser that used to work no longer does; forget it.
            *GOOD_BROWSER.lock().unwrap() = None;
        }
        if !(browser.is_some() && is_cookie_error(&err)) && self.first_err.is_none() {
            self.first_err = Some(err);
        }
        if !self.updated {
            self.updated = true;
            keep_fresh(tools, true).await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                _ = cancel.cancelled() => bail!("cancelled"),
            }
            self.plan.push(None);
        }
        if self.allow && !self.browsers_queued {
            self.browsers_queued = true;
            let skip = self.remembered.clone();
            self.plan.extend(browsers().into_iter().filter(|b| skip.as_deref() != Some(*b)).map(|b| Some(b.to_string())));
        }
        Ok(())
    }

    fn give_up(self) -> anyhow::Error {
        anyhow!(friendly(&self.first_err.unwrap_or_else(|| "yt-dlp failed".into())))
    }
}

pub async fn info(tools: &Tools, url: &str, cancel: &CancellationToken) -> Result<LinkInfo> {
    let mut fb = Fallbacks::start(tools).await;
    let out = loop {
        let Some(extra) = fb.next_args() else { return Err(fb.give_up()) };
        let mut c = ytdlp(tools)?;
        c.args(&extra);
        c.args(["-J", "--skip-download", url]);
        match run(c, cancel).await {
            Ok(o) => {
                fb.succeeded();
                break o;
            }
            Err(e) => fb.failed(tools, cancel, e).await?,
        }
    };
    let info: LinkInfo = serde_json::from_slice(&out.stdout).context("could not read the link's details")?;
    if info.is_live == Some(true) || info.live_status.as_deref() == Some("is_live") {
        return Err(anyhow!("This stream is still live. Cutdeck can clip it once the VOD is available."));
    }
    Ok(info)
}

fn friendly(e: &str) -> String {
    let l = e.to_lowercase();
    if l == "cancelled" {
        "cancelled".into()
    } else if l.contains("private video") {
        "This video is private.".into()
    } else if l.contains("sign in to confirm your age") || l.contains("age-restricted") {
        "This video is age-restricted. Sign in to YouTube in Firefox on this computer, then try again.".into()
    } else if l.contains("members-only") || l.contains("join this channel") {
        "This video is for channel members only.".into()
    } else if l.contains("sign in to confirm") || l.contains("not a bot") || l.contains("http error 429") || l.contains("too many requests") {
        "YouTube is blocking downloads from your network right now. Turn off any VPN or proxy and try again, or sign in to YouTube in Firefox on this computer. Cutdeck retried and updated its downloader already.".into()
    } else if l.contains("unsupported url") || l.contains("is not a valid url") {
        "Cutdeck can't read this link. Try a YouTube, Kick or Twitch video link.".into()
    } else if l.contains("video unavailable") || l.contains("has been removed") {
        "This video is unavailable. It may be removed, private or blocked in your country.".into()
    } else if l.contains("http error 403") {
        "YouTube refused the download. Cutdeck updated its downloader; try again in a minute.".into()
    } else if l.contains("getaddrinfo") || l.contains("timed out") || l.contains("connection") || l.contains("network is unreachable") {
        "Couldn't reach the video site. Check your internet connection and try again.".into()
    } else {
        e.to_string()
    }
}

fn parse_percent(line: &str) -> Option<f64> {
    let p = line.strip_prefix("cdprog:")?.trim().trim_end_matches('%').trim();
    p.parse::<f64>().ok().map(|v| v / 100.0)
}

/// Remove half-finished files from an earlier attempt.
fn clear_prefixed(dir: &Path, prefix: &str) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Download the audio only (small and fast) for analysis.
pub async fn download_audio(tools: &Tools, url: &str, dir: &Path, cancel: &CancellationToken, mut on: impl FnMut(f64)) -> Result<PathBuf> {
    let mut fb = Fallbacks::start(tools).await;
    let path = loop {
        let Some(extra) = fb.next_args() else { return Err(fb.give_up()) };
        clear_prefixed(dir, "source_audio.");
        let mut c = ytdlp(tools)?;
        c.args(&extra);
        c.args(["-f", "ba[ext=m4a]/ba/b", "--newline", "--progress-template", "download:cdprog:%(progress._percent_str)s"]);
        c.arg("-o").arg(dir.join("source_audio.%(ext)s"));
        c.args(["--print", "after_move:filepath", url]);
        let mut path = None;
        let r = run_lines(c, cancel, |is_err, line| {
            if let Some(p) = parse_percent(line) {
                on(p);
            } else if !is_err && !line.trim().is_empty() && Path::new(line.trim()).exists() {
                path = Some(PathBuf::from(line.trim()));
            }
        })
        .await;
        match r {
            Ok(()) => {
                fb.succeeded();
                break path;
            }
            Err(e) => fb.failed(tools, cancel, e).await?,
        }
    };
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
    let stem = out_stem.to_string_lossy().to_string();
    let name = out_stem.file_name().unwrap().to_string_lossy().to_string();
    let parent = out_stem.parent().unwrap();
    let mut fb = Fallbacks::start(tools).await;
    let path = loop {
        let Some(extra) = fb.next_args() else { return Err(fb.give_up()) };
        clear_prefixed(parent, &format!("{name}."));
        let mut c = ytdlp(tools)?;
        c.args(&extra);
        c.args(["-f", "bv*[height<=1080][vcodec!*=av01]+ba/bv*[height<=1080]+ba/b[height<=1080]/b"]);
        c.args(["--download-sections", &format!("*{start:.2}-{end:.2}"), "--force-keyframes-at-cuts"]);
        c.args(["--merge-output-format", "mp4", "--newline", "--progress-template", "download:cdprog:%(progress._percent_str)s"]);
        c.arg("-o").arg(format!("{stem}.%(ext)s"));
        c.args(["--print", "after_move:filepath", url]);
        let mut path = None;
        let r = run_lines(c, cancel, |is_err, line| {
            if let Some(p) = parse_percent(line) {
                on(p);
            } else if !is_err && Path::new(line.trim()).is_file() {
                path = Some(PathBuf::from(line.trim()));
            }
        })
        .await;
        match r {
            Ok(()) => {
                fb.succeeded();
                break path;
            }
            Err(e) => fb.failed(tools, cancel, e).await?,
        }
    };
    path.or_else(|| find_prefixed(parent, &format!("{name}.")))
        .ok_or_else(|| anyhow!("video download produced no file"))
}

/// Live-chat replay for past YouTube streams: message timestamps in seconds.
/// Best effort with a time limit; returns what was fetched.
pub async fn chat_times(tools: &Tools, url: &str, dir: &Path, limit_secs: u64) -> Vec<f64> {
    let Ok(mut c) = ytdlp(tools) else { return vec![] };
    if let Some(b) = GOOD_BROWSER.lock().unwrap().clone() {
        c.args(cookie_args(&b));
    }
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
    #[test]
    fn error_kinds() {
        assert!(is_final("cancelled"));
        assert!(is_final("ERROR: [youtube] x: Private video. Sign in if you've been granted access"));
        assert!(!is_final("ERROR: [youtube] x: Sign in to confirm you're not a bot"));
        assert!(!is_final("ERROR: unable to download video data: HTTP Error 403: Forbidden"));
        assert!(is_cookie_error("ERROR: Failed to decrypt with DPAPI"));
        assert!(is_cookie_error("ERROR: Could not copy Chrome cookie database"));
        assert!(!is_cookie_error("ERROR: Sign in to confirm you're not a bot"));
        assert!(friendly("Sign in to confirm you're not a bot").contains("VPN"));
        assert!(friendly("Sign in to confirm your age").contains("age-restricted"));
        assert_eq!(friendly("cancelled"), "cancelled");
    }

    /// A fake yt-dlp that is blocked until it is given Firefox cookies.
    #[cfg(windows)]
    #[tokio::test]
    async fn falls_back_to_browser_sign_in() {
        let root = std::env::temp_dir().join(format!("cutdeck-fb-{}", std::process::id()));
        let roam = root.join("roam");
        let local = root.join("local");
        std::fs::create_dir_all(roam.join("Mozilla").join("Firefox").join("Profiles")).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        let fake = root.join("yt-dlp.cmd");
        std::fs::write(
            &fake,
            "@echo off\r\n\
             echo %* | findstr /C:\"--update-to\" >nul && exit /b 0\r\n\
             echo %* | findstr /C:\"--cookies-from-browser firefox\" >nul && (echo {\"id\":\"x\",\"title\":\"ok\"} & exit /b 0)\r\n\
             echo ERROR: [youtube] x: Sign in to confirm you're not a bot 1>&2\r\n\
             exit /b 1\r\n",
        )
        .unwrap();
        // Safety: this is the only test that reads these variables.
        unsafe {
            std::env::set_var("APPDATA", &roam);
            std::env::set_var("LOCALAPPDATA", &local);
        }
        let tools = Tools {
            ffmpeg: root.join("ffmpeg.exe"),
            ffprobe: root.join("ffprobe.exe"),
            ytdlp: Some(fake),
            deno_dir: None,
            whisper: None,
            fonts_dir: root.clone(),
            face_model: root.clone(),
            models_dir: root.clone(),
        };
        let cancel = CancellationToken::new();

        set_browser_login(false);
        let e = info(&tools, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", &cancel).await.unwrap_err().to_string();
        assert!(e.contains("blocking downloads"), "{e}");

        set_browser_login(true);
        let i = info(&tools, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", &cancel).await.unwrap();
        assert_eq!(i.title.as_deref(), Some("ok"));
        assert_eq!(GOOD_BROWSER.lock().unwrap().as_deref(), Some("firefox"));
        // The next request goes straight to the browser that worked.
        let t = Instant::now();
        info(&tools, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", &cancel).await.unwrap();
        assert!(t.elapsed() < Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Real YouTube through the installed tools: `cargo test -p cutdeck-engine live_youtube -- --ignored`.
    #[cfg(windows)]
    #[tokio::test]
    #[ignore]
    async fn live_youtube() {
        let bin = PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.mohanbuilds.cutdeck").join("bin");
        let out = std::env::temp_dir().join("cutdeck-live");
        std::fs::create_dir_all(&out).unwrap();
        let tools = Tools {
            ffmpeg: bin.join("ffmpeg").join("ffmpeg.exe"),
            ffprobe: bin.join("ffmpeg").join("ffprobe.exe"),
            ytdlp: Some(bin.join("yt-dlp.exe")),
            deno_dir: Some(bin.join("deno")),
            whisper: None,
            fonts_dir: out.clone(),
            face_model: out.clone(),
            models_dir: out.clone(),
        };
        let cancel = CancellationToken::new();
        let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
        let i = info(&tools, url, &cancel).await.unwrap();
        assert!(i.duration.unwrap_or(0.0) > 5.0);
        let a = download_audio(&tools, url, &out, &cancel, |_| {}).await.unwrap();
        assert!(a.is_file());
        let v = download_section(&tools, url, 2.0, 6.0, &out.join("sec"), &cancel, |_| {}).await.unwrap();
        assert!(std::fs::metadata(&v).unwrap().len() > 10_000);
        let _ = std::fs::remove_dir_all(&out);
    }
}