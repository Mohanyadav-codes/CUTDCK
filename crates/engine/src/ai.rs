//! AI providers. Everything speaks the OpenAI-compatible API, so one client
//! covers Groq, Gemini, xAI (Grok), OpenAI, OpenRouter and local servers
//! (Ollama, LM Studio).

use crate::download::client;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub base_url: &'static str,
    pub key_url: &'static str,
    pub needs_key: bool,
    /// Cloud speech-to-text model on the same key, if the provider has one.
    pub stt_model: Option<&'static str>,
    pub free: bool,
    #[serde(skip)]
    pub prefs: &'static [&'static str],
    /// Minutes of transcript per picking request.
    #[serde(skip)]
    pub window_min: f64,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "groq",
        name: "Groq",
        tagline: "Free key. Does transcription and picking.",
        base_url: "https://api.groq.com/openai/v1",
        key_url: "https://console.groq.com/keys",
        needs_key: true,
        stt_model: Some("whisper-large-v3-turbo"),
        free: true,
        prefs: &["openai/gpt-oss-120b", "llama-3.3-70b-versatile", "qwen/qwen3", "openai/gpt-oss-20b", "llama"],
        window_min: 8.0,
    },
    Preset {
        id: "gemini",
        name: "Gemini",
        tagline: "Free tier from Google AI Studio.",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        key_url: "https://aistudio.google.com/apikey",
        needs_key: true,
        stt_model: None,
        free: true,
        prefs: &["gemini-2.5-flash", "gemini-flash-latest", "gemini-2.0-flash", "flash"],
        window_min: 40.0,
    },
    Preset {
        id: "xai",
        name: "Grok (xAI)",
        tagline: "Key from the xAI console.",
        base_url: "https://api.x.ai/v1",
        key_url: "https://console.x.ai",
        needs_key: true,
        stt_model: None,
        free: false,
        prefs: &["grok-4-fast-non-reasoning", "grok-4-fast", "grok-3-mini", "grok"],
        window_min: 30.0,
    },
    Preset {
        id: "openai",
        name: "OpenAI",
        tagline: "Paid key. Transcription included.",
        base_url: "https://api.openai.com/v1",
        key_url: "https://platform.openai.com/api-keys",
        needs_key: true,
        stt_model: Some("whisper-1"),
        free: false,
        prefs: &["gpt-4.1-mini", "gpt-4o-mini", "gpt-4.1", "gpt-4o"],
        window_min: 30.0,
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        tagline: "One key, many models.",
        base_url: "https://openrouter.ai/api/v1",
        key_url: "https://openrouter.ai/keys",
        needs_key: true,
        stt_model: None,
        free: false,
        prefs: &["google/gemini-2.5-flash", "meta-llama/llama-3.3-70b-instruct"],
        window_min: 20.0,
    },
    Preset {
        id: "local",
        name: "Local model",
        tagline: "Ollama or LM Studio on this computer.",
        base_url: "http://localhost:11434/v1",
        key_url: "https://ollama.com/download",
        needs_key: false,
        stt_model: None,
        free: true,
        prefs: &["qwen3", "qwen2.5", "llama3", "gemma"],
        window_min: 10.0,
    },
];

pub fn preset(id: &str) -> &'static Preset {
    PRESETS.iter().find(|p| p.id == id).unwrap_or(&PRESETS[0])
}

/// Saved AI choice (the key itself lives in the OS keychain).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// "cloud" (provider's speech-to-text) or "local" (whisper.cpp).
    pub stt: String,
    pub whisper_size: String,
    /// Optional separate key for speech-to-text (e.g. Groq while picking with Gemini).
    #[serde(default)]
    pub stt_provider: Option<String>,
}

#[derive(Clone)]
pub struct LlmClient {
    pub base_url: String,
    pub key: Option<String>,
    pub model: String,
    pub window_min: f64,
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelItem>,
}
#[derive(Deserialize)]
struct ModelItem {
    id: String,
}

fn auth(rb: reqwest::RequestBuilder, key: &Option<String>) -> reqwest::RequestBuilder {
    match key {
        Some(k) if !k.is_empty() => rb.bearer_auth(k),
        _ => rb,
    }
}

fn http_err(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message").and_then(|m| m.as_str().map(String::from)).or_else(|| e.as_str().map(String::from)))
                .or_else(|| v.get(0).and_then(|e| e.get("error")).and_then(|e| e.get("message")).and_then(|m| m.as_str().map(String::from)))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status.as_u16() {
        401 | 403 => anyhow!("The key was rejected ({status}). Check that it's copied in full. {msg}"),
        404 => anyhow!("Not found ({status}): {msg}"),
        429 => anyhow!("Rate limit reached: {msg}"),
        _ => anyhow!("AI request failed ({status}): {msg}"),
    }
}

/// List models and choose the best one for picking.
pub async fn list_models(base_url: &str, key: &Option<String>) -> Result<Vec<String>> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let resp = auth(client().get(&url), key).timeout(Duration::from_secs(20)).send().await.map_err(|e| {
        if base_url.contains("localhost") || base_url.contains("127.0.0.1") {
            anyhow!("No local model server answered at {base_url}. Start Ollama or LM Studio first.")
        } else {
            anyhow!("Could not reach {base_url}: {e}")
        }
    })?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(http_err(status, &body));
    }
    let list: ModelList = serde_json::from_str(&body).map_err(|_| anyhow!("Unexpected answer from {url}"))?;
    Ok(list.data.into_iter().map(|m| m.id.trim_start_matches("models/").to_string()).collect())
}

pub fn choose_model(models: &[String], prefs: &[&str]) -> Option<String> {
    let bad = ["whisper", "tts", "embed", "guard", "orpheus", "distil", "image", "vision-preview", "audio", "aqa", "imagen", "veo", "live", "native"];
    let usable: Vec<&String> = models.iter().filter(|m| !bad.iter().any(|b| m.to_lowercase().contains(b))).collect();
    for p in prefs {
        if let Some(m) = usable.iter().find(|m| m.as_str() == *p) {
            return Some(m.to_string());
        }
    }
    for p in prefs {
        if let Some(m) = usable.iter().find(|m| m.contains(p)) {
            return Some(m.to_string());
        }
    }
    usable.first().map(|m| m.to_string())
}

impl LlmClient {
    /// Ask for a JSON object. Retries on rate limits with the server's wait time.
    pub async fn json(&self, system: &str, user: &str, cancel: &CancellationToken) -> Result<Value> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut use_format = true;
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let mut body = json!({
                "model": self.model,
                "temperature": 0.4,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
            });
            if use_format {
                body["response_format"] = json!({"type": "json_object"});
            }
            let req = auth(client().post(&url), &self.key).timeout(Duration::from_secs(180)).json(&body);
            let resp = tokio::select! {
                r = req.send() => r,
                _ = cancel.cancelled() => bail!("cancelled"),
            };
            let resp = match resp {
                Ok(r) => r,
                Err(e) if attempt < 4 => {
                    tracing::warn!("llm request error: {e}");
                    sleep(Duration::from_secs(3 * attempt as u64), cancel).await?;
                    continue;
                }
                Err(e) => bail!("AI request failed: {e}"),
            };
            let status = resp.status();
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<f64>().ok());
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let v: Value = serde_json::from_str(&text).map_err(|_| anyhow!("unexpected AI answer"))?;
                let content = v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
                match parse_json_loose(&content) {
                    Some(j) => return Ok(j),
                    None if attempt < 3 => continue,
                    None => bail!("the AI didn't return valid JSON"),
                }
            }
            if status.as_u16() == 429 && attempt < 8 {
                let wait = retry_after.unwrap_or(2f64.powi(attempt as i32).min(60.0)).clamp(1.0, 90.0);
                tracing::info!("rate limited, waiting {wait}s");
                sleep(Duration::from_secs_f64(wait + 0.5), cancel).await?;
                continue;
            }
            if status.as_u16() == 400 && use_format && text.to_lowercase().contains("response_format") {
                use_format = false;
                continue;
            }
            if status.is_server_error() && attempt < 4 {
                sleep(Duration::from_secs(2 * attempt as u64), cancel).await?;
                continue;
            }
            return Err(http_err(status, &text));
        }
    }
}

async fn sleep(d: Duration, cancel: &CancellationToken) -> Result<()> {
    tokio::select! {
        _ = tokio::time::sleep(d) => Ok(()),
        _ = cancel.cancelled() => bail!("cancelled"),
    }
}

/// Parse JSON that may be wrapped in code fences or prose.
pub fn parse_json_loose(s: &str) -> Option<Value> {
    let t = s.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let start = t.find('{')?;
    let end = t.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&t[start..=end]).ok()
}

/// Cloud speech-to-text (OpenAI-compatible `/audio/transcriptions`).
#[derive(Clone)]
pub struct CloudStt {
    pub base_url: String,
    pub key: String,
    pub model: String,
}

#[derive(Deserialize)]
struct SttWord {
    word: String,
    start: f64,
    end: f64,
}
#[derive(Deserialize)]
struct SttResp {
    #[serde(default)]
    words: Vec<SttWord>,
}

impl CloudStt {
    pub async fn transcribe_file(&self, path: &std::path::Path, cancel: &CancellationToken) -> Result<Vec<crate::types::Word>> {
        let url = format!("{}/audio/transcriptions", self.base_url.trim_end_matches('/'));
        let bytes = tokio::fs::read(path).await?;
        let fname = path.file_name().unwrap().to_string_lossy().to_string();
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let part = reqwest::multipart::Part::bytes(bytes.clone()).file_name(fname.clone()).mime_str("audio/ogg")?;
            let form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("model", self.model.clone())
                .text("response_format", "verbose_json")
                .text("timestamp_granularities[]", "word")
                .text("language", "en")
                .text("temperature", "0");
            let req = client().post(&url).bearer_auth(&self.key).timeout(Duration::from_secs(300)).multipart(form);
            let resp = tokio::select! {
                r = req.send() => r,
                _ = cancel.cancelled() => bail!("cancelled"),
            };
            let resp = match resp {
                Ok(r) => r,
                Err(e) if attempt < 4 => {
                    tracing::warn!("stt request error: {e}");
                    sleep(Duration::from_secs(3 * attempt as u64), cancel).await?;
                    continue;
                }
                Err(e) => bail!("transcription request failed: {e}"),
            };
            let status = resp.status();
            let retry_after = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok());
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let r: SttResp = serde_json::from_str(&text).map_err(|_| anyhow!("unexpected transcription answer"))?;
                return Ok(r.words.into_iter().map(|w| crate::types::Word { w: w.word.trim().to_string(), s: w.start, e: w.end }).collect());
            }
            if status.as_u16() == 429 && attempt < 10 {
                // Audio-seconds-per-hour limits can ask for long waits.
                let wait = retry_after.unwrap_or(20.0 * attempt as f64).clamp(2.0, 600.0);
                tracing::info!("transcription rate limited, waiting {wait}s");
                sleep(Duration::from_secs_f64(wait + 0.5), cancel).await?;
                continue;
            }
            if status.is_server_error() && attempt < 4 {
                sleep(Duration::from_secs(3 * attempt as u64), cancel).await?;
                continue;
            }
            return Err(http_err(status, &text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loose_json() {
        assert!(parse_json_loose("```json\n{\"a\":1}\n```").is_some());
        assert!(parse_json_loose("Here: {\"clips\": []} done").is_some());
        assert!(parse_json_loose("nope").is_none());
    }
    #[test]
    fn model_choice() {
        let m: Vec<String> = ["whisper-large-v3", "llama-3.3-70b-versatile", "openai/gpt-oss-120b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choose_model(&m, preset("groq").prefs).unwrap(), "openai/gpt-oss-120b");
        let g: Vec<String> = ["embedding-001", "gemini-2.5-flash", "gemini-2.5-pro"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choose_model(&g, preset("gemini").prefs).unwrap(), "gemini-2.5-flash");
    }
}
