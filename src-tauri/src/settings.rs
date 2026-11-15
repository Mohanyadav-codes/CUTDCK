//! App settings (JSON in the config folder) and API keys (OS keychain).

use cutdeck_engine::ai::AiConfig;
use cutdeck_engine::project::{read_json, write_json};
use cutdeck_engine::types::JobSettings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub onboarded: bool,
    pub ai: Option<AiConfig>,
    pub job: JobSettings,
    pub export_dir: String,
    pub launch_at_login: bool,
    /// Max minutes of audio to transcribe for very long streams.
    pub listen_budget_min: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            ai: None,
            job: JobSettings::default(),
            export_dir: default_export_dir().to_string_lossy().to_string(),
            launch_at_login: false,
            listen_budget_min: 60.0,
        }
    }
}

fn default_export_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(if cfg!(windows) { "Videos" } else { "Movies" }).join("Cutdeck")
}

pub fn load(path: &Path) -> Settings {
    read_json(path).unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) -> anyhow::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    write_json(path, s)
}

const SERVICE: &str = "com.mohanbuilds.cutdeck";

/// Keys go to Windows Credential Manager / macOS Keychain. If the keychain is
/// unavailable, fall back to a file in the app's private data folder.
pub struct Keys {
    pub fallback: PathBuf,
}

impl Keys {
    pub fn get(&self, provider: &str) -> Option<String> {
        if let Ok(e) = keyring::Entry::new(SERVICE, provider) {
            if let Ok(k) = e.get_password() {
                return Some(k);
            }
        }
        let m: serde_json::Map<String, serde_json::Value> = read_json(&self.fallback).unwrap_or_default();
        m.get(provider).and_then(|v| v.as_str()).map(String::from)
    }

    pub fn set(&self, provider: &str, key: &str) -> anyhow::Result<()> {
        if let Ok(e) = keyring::Entry::new(SERVICE, provider) {
            if e.set_password(key).is_ok() && e.get_password().ok().as_deref() == Some(key) {
                return Ok(());
            }
        }
        let mut m: serde_json::Map<String, serde_json::Value> = read_json(&self.fallback).unwrap_or_default();
        m.insert(provider.into(), key.into());
        write_json(&self.fallback, &m)
    }

    pub fn delete(&self, provider: &str) {
        if let Ok(e) = keyring::Entry::new(SERVICE, provider) {
            let _ = e.delete_credential();
        }
        let mut m: serde_json::Map<String, serde_json::Value> = read_json(&self.fallback).unwrap_or_default();
        if m.remove(provider).is_some() {
            let _ = write_json(&self.fallback, &m);
        }
    }
}
