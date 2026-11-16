//! Runtime tools Cutdeck downloads on first run: FFmpeg, yt-dlp, Deno (needed by
//! yt-dlp for YouTube), and optionally whisper.cpp for on-device transcription.

use crate::download::{client, download, extract_zip};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

/// Resolved tool paths used by the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ytdlp: Option<PathBuf>,
    pub deno_dir: Option<PathBuf>,
    pub whisper: Option<PathBuf>,
    pub fonts_dir: PathBuf,
    pub face_model: PathBuf,
    pub models_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DepsStatus {
    pub ffmpeg: bool,
    pub ytdlp: bool,
    pub deno: bool,
    pub whisper: bool,
    pub whisper_models: Vec<String>,
}

impl DepsStatus {
    pub fn ready(&self) -> bool {
        self.ffmpeg && self.ytdlp && self.deno
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(format!("{name}{EXE}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub struct Layout {
    pub bin: PathBuf,
    pub models: PathBuf,
}

impl Layout {
    pub fn new(data_dir: &Path) -> Self {
        Self { bin: data_dir.join("bin"), models: data_dir.join("models") }
    }
    fn ffmpeg(&self) -> PathBuf {
        self.bin.join("ffmpeg").join(format!("ffmpeg{EXE}"))
    }
    fn ffprobe(&self) -> PathBuf {
        self.bin.join("ffmpeg").join(format!("ffprobe{EXE}"))
    }
    fn ytdlp(&self) -> PathBuf {
        self.bin.join(format!("yt-dlp{EXE}"))
    }
    fn deno_dir(&self) -> PathBuf {
        self.bin.join("deno")
    }
    fn whisper(&self) -> PathBuf {
        self.bin.join("whisper").join(format!("whisper-cli{EXE}"))
    }

    pub fn status(&self) -> DepsStatus {
        let mut models = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.models) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("ggml-") && n.ends_with(".bin") {
                    models.push(n);
                }
            }
        }
        DepsStatus {
            ffmpeg: self.ffmpeg().is_file() && self.ffprobe().is_file(),
            ytdlp: self.ytdlp().is_file(),
            deno: self.deno_dir().join(format!("deno{EXE}")).is_file(),
            whisper: self.whisper().is_file(),
            whisper_models: models,
        }
    }

    /// Tools from the data dir, falling back to PATH (handy in development).
    pub fn tools(&self, fonts_dir: PathBuf, face_model: PathBuf) -> Result<Tools> {
        let pick = |own: PathBuf, name: &str| -> Option<PathBuf> {
            if own.is_file() { Some(own) } else { which(name) }
        };
        let ffmpeg = pick(self.ffmpeg(), "ffmpeg").ok_or_else(|| anyhow!("FFmpeg is not installed yet"))?;
        let ffprobe = pick(self.ffprobe(), "ffprobe").ok_or_else(|| anyhow!("FFprobe is not installed yet"))?;
        let deno_dir = if self.deno_dir().join(format!("deno{EXE}")).is_file() {
            Some(self.deno_dir())
        } else {
            which("deno").and_then(|p| p.parent().map(|d| d.to_path_buf()))
        };
        Ok(Tools {
            ffmpeg,
            ffprobe,
            ytdlp: pick(self.ytdlp(), "yt-dlp"),
            deno_dir,
            whisper: pick(self.whisper(), "whisper-cli"),
            fonts_dir,
            face_model,
            models_dir: self.models.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DepProgress {
    pub item: String,
    pub label: String,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}
#[derive(Deserialize, Clone)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

async fn gh_latest(repo: &str) -> Result<GhRelease> {
    let r = client()
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("GitHub release lookup for {repo}"))?;
    Ok(r.json().await?)
}

async fn gh_recent(repo: &str) -> Result<Vec<GhRelease>> {
    let r = client()
        .get(format!("https://api.github.com/repos/{repo}/releases?per_page=8"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;
    Ok(r.json().await?)
}

async fn fetch_text(url: &str) -> Result<String> {
    Ok(client().get(url).send().await?.error_for_status()?.text().await?)
}

/// Find the SHA-256 for `file` in a checksum listing ("<hash>  <name>" lines),
/// or the first 64-hex token when the listing is for a single file.
fn find_sha(listing: &str, file: Option<&str>) -> Option<String> {
    let is_hex = |t: &str| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit());
    for line in listing.lines() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        if let Some(h) = toks.iter().find(|t| is_hex(t)) {
            match file {
                Some(f) if !line.contains(f) => continue,
                _ => return Some(h.to_lowercase()),
            }
        }
    }
    None
}

fn temp_dir(layout: &Layout) -> PathBuf {
    layout.bin.join(".download")
}

/// Install everything missing. Safe to call repeatedly.
pub async fn install(
    layout: &Layout,
    cancel: &CancellationToken,
    on: &(dyn Fn(DepProgress) + Send + Sync),
) -> Result<()> {
    let st = layout.status();
    std::fs::create_dir_all(&layout.bin)?;
    let tmp = temp_dir(layout);
    std::fs::create_dir_all(&tmp)?;
    let report = |item: &str, label: &str, done: u64, total: u64, finished: bool| {
        on(DepProgress { item: item.into(), label: label.into(), done, total, finished })
    };

    if !st.ffmpeg {
        let label = "FFmpeg (video engine)";
        if cfg!(windows) {
            let rel = gh_latest("BtbN/FFmpeg-Builds").await?;
            let mut cands: Vec<(u32, u32, GhAsset)> = rel
                .assets
                .iter()
                .filter_map(|a| {
                    let re = regex::Regex::new(r"^ffmpeg-n(\d+)\.(\d+)-latest-win64-gpl-shared-[\d.]+\.zip$").ok()?;
                    let c = re.captures(&a.name)?;
                    Some((c[1].parse().ok()?, c[2].parse().ok()?, a.clone()))
                })
                .collect();
            cands.sort_by_key(|c| (c.0, c.1));
            let asset = cands.pop().map(|c| c.2).ok_or_else(|| anyhow!("no FFmpeg build found"))?;
            let sums = rel.assets.iter().find(|a| a.name == "checksums.sha256");
            let sha = match sums {
                Some(s) => find_sha(&fetch_text(&s.browser_download_url).await?, Some(&asset.name)),
                None => None,
            };
            let zip = tmp.join("ffmpeg.zip");
            download(&asset.browser_download_url, &zip, sha.as_deref(), cancel, |d, t| report("ffmpeg", label, d, t, false)).await?;
            let out = layout.bin.join("ffmpeg");
            let z = zip.clone();
            let o = out.clone();
            tokio::task::spawn_blocking(move || extract_zip(&z, &o, &["ffmpeg.exe", "ffprobe.exe", ".dll"])).await??;
            let _ = std::fs::remove_file(&zip);
        } else {
            let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
            let out = layout.bin.join("ffmpeg");
            for tool in ["ffmpeg", "ffprobe"] {
                let url = format!("https://ffmpeg.martin-riedl.de/redirect/latest/macos/{arch}/release/{tool}.zip");
                let zip = tmp.join(format!("{tool}.zip"));
                download(&url, &zip, None, cancel, |d, t| report("ffmpeg", label, d, t, false)).await?;
                let z = zip.clone();
                let o = out.clone();
                let name = tool.to_string();
                tokio::task::spawn_blocking(move || extract_zip(&z, &o, &[name.as_str()])).await??;
                let _ = std::fs::remove_file(&zip);
            }
        }
        report("ffmpeg", label, 1, 1, true);
    }

    if !st.ytdlp {
        let label = "yt-dlp (link downloader)";
        // Nightly is yt-dlp's recommended channel; it gets YouTube fixes within a day.
        let rel = match gh_latest("yt-dlp/yt-dlp-nightly-builds").await {
            Ok(r) if r.assets.iter().any(|a| a.name == if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp_macos" }) => r,
            _ => gh_latest("yt-dlp/yt-dlp").await?,
        };
        let name = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp_macos" };
        let asset = rel.assets.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("no yt-dlp build found"))?;
        let sums = rel.assets.iter().find(|a| a.name == "SHA2-256SUMS");
        let sha = match sums {
            Some(s) => find_sha(&fetch_text(&s.browser_download_url).await?, Some(name)),
            None => None,
        };
        download(&asset.browser_download_url, &layout.ytdlp(), sha.as_deref(), cancel, |d, t| report("ytdlp", label, d, t, false)).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(layout.ytdlp(), std::fs::Permissions::from_mode(0o755))?;
        }
        report("ytdlp", label, 1, 1, true);
    }

    if !st.deno {
        let label = "Deno (YouTube helper)";
        let triple = if cfg!(windows) {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        let rel = gh_latest("denoland/deno").await?;
        let zname = format!("deno-{triple}.zip");
        let asset = rel.assets.iter().find(|a| a.name == zname).ok_or_else(|| anyhow!("no Deno build found"))?;
        let sums = rel.assets.iter().find(|a| a.name == format!("{zname}.sha256sum"));
        let sha = match sums {
            Some(s) => find_sha(&fetch_text(&s.browser_download_url).await?, None),
            None => None,
        };
        let zip = tmp.join("deno.zip");
        download(&asset.browser_download_url, &zip, sha.as_deref(), cancel, |d, t| report("deno", label, d, t, false)).await?;
        let z = zip.clone();
        let o = layout.deno_dir();
        tokio::task::spawn_blocking(move || extract_zip(&z, &o, &[if cfg!(windows) { "deno.exe" } else { "deno" }])).await??;
        let _ = std::fs::remove_file(&zip);
        let _ = rel.tag_name;
        report("deno", label, 1, 1, true);
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// Whisper model choices for on-device transcription.
pub fn whisper_model_file(size: &str) -> &'static str {
    match size {
        "small" => "ggml-small.en-q5_1.bin",
        _ => "ggml-base.en-q5_1.bin",
    }
}

/// Install whisper.cpp and one English model (optional, user-initiated).
pub async fn install_whisper(
    layout: &Layout,
    size: &str,
    cancel: &CancellationToken,
    on: &(dyn Fn(DepProgress) + Send + Sync),
) -> Result<()> {
    let tmp = temp_dir(layout);
    std::fs::create_dir_all(&tmp)?;
    if !layout.whisper().is_file() {
        if !cfg!(windows) {
            bail!("On-device Whisper isn't packaged for macOS yet. Use a Groq or OpenAI key for transcription.");
        }
        let label = "Whisper engine";
        let rels = gh_recent("ggml-org/whisper.cpp").await?;
        let asset = rels
            .iter()
            .flat_map(|r| r.assets.iter())
            .find(|a| a.name == "whisper-blas-bin-x64.zip")
            .cloned()
            .ok_or_else(|| anyhow!("no whisper.cpp Windows build found"))?;
        let zip = tmp.join("whisper.zip");
        download(&asset.browser_download_url, &zip, None, cancel, |d, t| {
            on(DepProgress { item: "whisper".into(), label: label.into(), done: d, total: t, finished: false })
        })
        .await?;
        let z = zip.clone();
        let o = layout.bin.join("whisper");
        tokio::task::spawn_blocking(move || extract_zip(&z, &o, &[".exe", ".dll"])).await??;
        let _ = std::fs::remove_file(&zip);
        if !layout.whisper().is_file() {
            bail!("whisper-cli was not found in the downloaded package");
        }
    }
    let file = whisper_model_file(size);
    let dest = layout.models.join(file);
    if !dest.is_file() {
        let url = format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{file}");
        let label = if size == "small" { "Speech model (small, 190 MB)" } else { "Speech model (base, 60 MB)" };
        download(&url, &dest, None, cancel, |d, t| {
            on(DepProgress { item: "whisper_model".into(), label: label.into(), done: d, total: t, finished: false })
        })
        .await?;
    }
    on(DepProgress { item: "whisper".into(), label: "Whisper".into(), done: 1, total: 1, finished: true });
    Ok(())
}

/// Ask yt-dlp to update itself (YouTube changes often). Errors are ignored by callers.
pub async fn update_ytdlp(tools: &Tools) -> Result<()> {
    if let Some(y) = &tools.ytdlp {
        let mut c = crate::proc::command(y);
        c.args(["--update-to", "nightly"]);
        crate::proc::run(c, &CancellationToken::new()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha_listing() {
        let l = "aaaa  x\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  yt-dlp.exe\n";
        assert_eq!(find_sha(l, Some("yt-dlp.exe")).unwrap().len(), 64);
        assert!(find_sha(l, Some("nope")).is_none());
        let single = "Algorithm Hash Path\nSHA256 0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF C:\\a\\deno.zip";
        assert!(find_sha(single, None).is_some());
    }
}
