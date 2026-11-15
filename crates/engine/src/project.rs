//! Project folders and safe concurrent updates of `project.json`.

use crate::types::Project;
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct Store {
    pub root: PathBuf,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

pub fn project_id(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    if !crate::source::is_url(input) {
        // Same path but a different file (edited or replaced) is a new project.
        if let Ok(m) = std::fs::metadata(input) {
            h.update(m.len().to_le_bytes());
            if let Ok(t) = m.modified() {
                if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                    h.update(d.as_secs().to_le_bytes());
                }
            }
        }
    }
    hex::encode(&h.finalize()[..6])
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let b = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_slice(&b)?)
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&root);
        Self { root, locks: Mutex::new(HashMap::new()) }
    }

    pub fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    pub fn clip_dir(&self, id: &str, clip: &str) -> PathBuf {
        self.root.join(id).join("clips").join(clip)
    }

    fn lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().unwrap().entry(id.to_string()).or_default().clone()
    }

    pub fn load(&self, id: &str) -> Result<Project> {
        read_json(&self.dir(id).join("project.json"))
    }

    /// Load, change and save under the project's lock.
    pub async fn update<R>(&self, id: &str, f: impl FnOnce(&mut Project) -> R) -> Result<(R, Project)> {
        let l = self.lock(id);
        let _g = l.lock().await;
        let mut p = self.load(id)?;
        let r = f(&mut p);
        write_json(&self.dir(id).join("project.json"), &p)?;
        Ok((r, p))
    }

    pub async fn save_new(&self, p: &Project) -> Result<()> {
        let l = self.lock(&p.id);
        let _g = l.lock().await;
        std::fs::create_dir_all(self.dir(&p.id))?;
        write_json(&self.dir(&p.id).join("project.json"), p)
    }

    pub fn list(&self) -> Vec<Project> {
        let mut out: Vec<Project> = std::fs::read_dir(&self.root)
            .map(|rd| rd.flatten().filter_map(|e| read_json(&e.path().join("project.json")).ok()).collect())
            .unwrap_or_default();
        out.sort_by(|a, b| b.created.cmp(&a.created));
        out
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let l = self.lock(id);
        let _g = l.lock().await;
        let d = self.dir(id);
        if d.starts_with(&self.root) && d != self.root {
            std::fs::remove_dir_all(d)?;
        }
        Ok(())
    }
}
