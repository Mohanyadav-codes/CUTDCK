//! Resumable HTTP downloads with SHA-256 verification.

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Cutdeck/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("http client")
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Download `url` to `dest`. Partial data lives in `dest.part` and resumes with
/// an HTTP Range request. `on_progress(done, total)`; total is 0 when unknown.
pub async fn download(
    url: &str,
    dest: &Path,
    sha256: Option<&str>,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf> {
    if let Some(p) = dest.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()
    ));
    let mut have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    let http = client();
    let mut req = http.get(url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await.with_context(|| format!("download {url}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The part file is already complete (or bad). Start over.
        let _ = tokio::fs::remove_file(&part).await;
        return Box::pin(download(url, dest, sha256, cancel, on_progress)).await;
    }
    if !status.is_success() {
        bail!("download failed: HTTP {status} for {url}");
    }
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resumed {
        have = 0;
    }
    let total = resp.content_length().map(|l| l + have).unwrap_or(0);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)
        .await?;
    let mut stream = resp.bytes_stream();
    let mut done = have;
    on_progress(done, total);
    loop {
        tokio::select! {
            chunk = stream.next() => match chunk {
                Some(c) => {
                    let c = c.context("connection dropped during download")?;
                    file.write_all(&c).await?;
                    done += c.len() as u64;
                    on_progress(done, total);
                }
                None => break,
            },
            _ = cancel.cancelled() => { file.flush().await?; bail!("cancelled"); }
        }
    }
    file.flush().await?;
    drop(file);
    if let Some(want) = sha256 {
        let p = part.clone();
        let got = tokio::task::spawn_blocking(move || sha256_file(&p)).await??;
        if !got.eq_ignore_ascii_case(want.trim()) {
            let _ = tokio::fs::remove_file(&part).await;
            bail!("checksum mismatch for {url}");
        }
    }
    if dest.exists() {
        tokio::fs::remove_file(dest).await?;
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(dest.to_path_buf())
}

/// Extract files from a zip whose names end with one of `wanted` into `out_dir`
/// (flattened). Returns the extracted paths.
pub fn extract_zip(zip_path: &Path, out_dir: &Path, wanted: &[&str]) -> Result<Vec<PathBuf>> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f)?;
    std::fs::create_dir_all(out_dir)?;
    let mut out = Vec::new();
    for i in 0..z.len() {
        let mut entry = z.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.replace('\\', "/");
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if wanted.is_empty() || wanted.iter().any(|w| base.eq_ignore_ascii_case(w) || base.to_lowercase().ends_with(&w.to_lowercase())) {
            let dest = out_dir.join(&base);
            let mut o = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut o)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
            }
            out.push(dest);
        }
    }
    Ok(out)
}
