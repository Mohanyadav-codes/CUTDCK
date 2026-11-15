//! Speech to text with word timings, from a cloud provider or whisper.cpp.

use crate::ai::CloudStt;
use crate::deps::Tools;
use crate::media::encode_opus_slice;
use crate::proc::{command, run_lines};
use crate::types::Word;
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub enum Stt {
    Cloud(CloudStt),
    Local { bin: PathBuf, model: PathBuf },
}

const CHUNK: f64 = 900.0; // 15 min of Opus at 32 kb/s is about 3.6 MB
const OVERLAP: f64 = 4.0;

/// Split `regions` into upload-sized chunks with a small overlap.
pub fn chunks(regions: &[(f64, f64)]) -> Vec<(f64, f64, f64, f64)> {
    // (chunk_start, chunk_end, keep_from, keep_to)
    let mut out = Vec::new();
    for &(s, e) in regions {
        let mut t = s;
        while t < e - 0.5 {
            let end = (t + CHUNK).min(e);
            let cs = if t > s { t - OVERLAP } else { t };
            let keep_from = if t > s { t } else { s };
            out.push((cs, end, keep_from, end));
            t = end;
        }
    }
    out
}

pub async fn transcribe(
    stt: &Stt,
    tools: &Tools,
    wav: &Path,
    regions: &[(f64, f64)],
    work: &Path,
    cancel: &CancellationToken,
    on: &(dyn Fn(f64) + Send + Sync),
) -> Result<Vec<Word>> {
    let total: f64 = regions.iter().map(|r| r.1 - r.0).sum::<f64>().max(1.0);
    let mut words: Vec<Word> = Vec::new();
    let mut done = 0.0;
    match stt {
        Stt::Cloud(c) => {
            for (i, (cs, ce, kf, kt)) in chunks(regions).into_iter().enumerate() {
                let f = work.join(format!("stt_{i:03}.ogg"));
                encode_opus_slice(tools, wav, cs, ce - cs, &f, cancel).await?;
                let ws = c.transcribe_file(&f, cancel).await.context("transcription failed")?;
                let _ = std::fs::remove_file(&f);
                for mut w in ws {
                    w.s += cs;
                    w.e += cs;
                    let mid = (w.s + w.e) / 2.0;
                    if mid >= kf && mid < kt && !w.w.is_empty() {
                        words.push(w);
                    }
                }
                done += kt - kf;
                on(done / total);
            }
        }
        Stt::Local { bin, model } => {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
            for (i, &(s, e)) in regions.iter().enumerate() {
                let prefix = work.join(format!("whisper_{i:03}"));
                let mut c = command(bin);
                c.arg("-m").arg(model).arg("-f").arg(wav);
                c.args(["-l", "en", "-ml", "1", "-sow", "-oj", "-pp", "-np", "-t", &threads.to_string()]);
                c.args(["-ot", &((s * 1000.0) as i64).to_string(), "-d", &(((e - s) * 1000.0) as i64).to_string()]);
                c.arg("-of").arg(&prefix);
                let base = done;
                let span = e - s;
                run_lines(c, cancel, |_, line| {
                    if let Some(p) = line.split("progress =").nth(1) {
                        if let Ok(v) = p.trim().trim_end_matches('%').trim().parse::<f64>() {
                            on((base + span * v / 100.0) / total);
                        }
                    }
                })
                .await?;
                let json_path = prefix.with_extension("json");
                let text = std::fs::read_to_string(&json_path).with_context(|| "whisper produced no output")?;
                let mut ws = parse_whisper_json(&text)?;
                // Offsets are absolute in recent builds; older builds report them relative to -ot.
                if let Some(first) = ws.first() {
                    if s > 5.0 && first.s < s - 2.0 {
                        for w in ws.iter_mut() {
                            w.s += s;
                            w.e += s;
                        }
                    }
                }
                words.extend(ws.into_iter().filter(|w| w.s >= s - 0.5 && w.s < e + 0.5));
                let _ = std::fs::remove_file(json_path);
                done += span;
                on(done / total);
            }
        }
    }
    words.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap());
    Ok(clean_words(words))
}

fn parse_whisper_json(text: &str) -> Result<Vec<Word>> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    let arr = v["transcription"].as_array().ok_or_else(|| anyhow!("unexpected whisper output"))?;
    let mut out: Vec<Word> = Vec::new();
    for seg in arr {
        let t = seg["text"].as_str().unwrap_or("").to_string();
        let from = seg["offsets"]["from"].as_f64().unwrap_or(0.0) / 1000.0;
        let to = seg["offsets"]["to"].as_f64().unwrap_or(0.0) / 1000.0;
        let trimmed = t.trim();
        if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with('(') {
            continue;
        }
        // Punctuation-only tokens attach to the previous word.
        if !t.starts_with(' ') && !out.is_empty() && trimmed.chars().all(|c| !c.is_alphanumeric()) {
            let last = out.last_mut().unwrap();
            last.w.push_str(trimmed);
            last.e = last.e.max(to);
            continue;
        }
        if !t.starts_with(' ') && !out.is_empty() {
            // Continuation of a word split into several tokens.
            let last = out.last_mut().unwrap();
            last.w.push_str(trimmed);
            last.e = last.e.max(to);
            continue;
        }
        out.push(Word { w: trimmed.to_string(), s: from, e: to.max(from + 0.05) });
    }
    Ok(out)
}

/// Remove obvious hallucinations: repeated runs and zero-length junk.
pub fn clean_words(words: Vec<Word>) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    let mut run = 0;
    for w in words {
        if w.e < w.s {
            continue;
        }
        if let Some(last) = out.last() {
            if last.w.eq_ignore_ascii_case(&w.w) {
                run += 1;
                if run >= 3 {
                    continue;
                }
            } else {
                run = 0;
            }
            if w.s < last.s {
                continue;
            }
        }
        out.push(w);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunking_covers_regions() {
        let c = chunks(&[(0.0, 2000.0)]);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].2, 0.0);
        assert_eq!(c[1].0, 900.0 - OVERLAP);
        assert_eq!(c[1].2, 900.0);
        assert_eq!(c.last().unwrap().3, 2000.0);
    }
    #[test]
    fn whisper_json() {
        let j = r#"{"transcription":[{"offsets":{"from":0,"to":300},"text":" Hello"},{"offsets":{"from":300,"to":320},"text":","},{"offsets":{"from":400,"to":700},"text":" world"},{"offsets":{"from":700,"to":800},"text":"wide"}]}"#;
        let w = parse_whisper_json(j).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].w, "Hello,");
        assert_eq!(w[1].w, "worldwide");
    }
}
