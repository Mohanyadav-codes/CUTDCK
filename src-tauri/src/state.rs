//! Shared app state, the job queue and the export queue.

use crate::settings::{Keys, Settings};
use anyhow::{Result, anyhow};
use cutdeck_engine::ai::{self, CloudStt, LlmClient};
use cutdeck_engine::deps::{self, Layout as DepLayout};
use cutdeck_engine::transcribe::Stt;
use cutdeck_engine::types::ClipStatus;
use cutdeck_engine::{CancellationToken, Emit, Engine, Event, JobCtx};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl HitRect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Default)]
pub struct Queue {
    pub pending: VecDeque<String>,
    pub current: Option<(String, CancellationToken)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueState {
    pub current: Option<String>,
    pub pending: Vec<String>,
    pub exporting: Option<(String, String)>,
    pub export_pending: Vec<(String, String)>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub resources: PathBuf,
    pub settings_path: PathBuf,
    pub settings: Mutex<Settings>,
    pub keys: Keys,
    pub engine: tokio::sync::RwLock<Option<Arc<Engine>>>,
    pub queue: Mutex<Queue>,
    pub queue_wake: tokio::sync::Notify,
    pub exports: Mutex<(VecDeque<(String, String)>, Option<(String, String)>)>,
    pub export_wake: tokio::sync::Notify,
    pub hit: Mutex<Vec<HitRect>>,
    pub drop_zone: Mutex<Option<HitRect>>,
    pub deps_busy: AtomicBool,
}

impl AppState {
    pub fn layout(&self) -> DepLayout {
        DepLayout::new(&self.data_dir)
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, s: Settings) -> Result<()> {
        crate::settings::save(&self.settings_path, &s)?;
        *self.settings.lock().unwrap() = s;
        Ok(())
    }

    /// The engine, created once the tools are installed.
    pub async fn engine(&self) -> Result<Arc<Engine>> {
        if let Some(e) = self.engine.read().await.as_ref() {
            return Ok(e.clone());
        }
        let tools = self.layout().tools(self.resources.join("fonts"), self.resources.join("models").join("seeta_fd_frontal_v1.0.bin"))?;
        let e = Arc::new(Engine::new(tools, self.data_dir.join("projects")));
        *self.engine.write().await = Some(e.clone());
        Ok(e)
    }

    pub async fn reset_engine(&self) {
        *self.engine.write().await = None;
    }

    /// Build the AI side of a job from settings and stored keys.
    pub fn job_ctx(&self) -> Result<JobCtx> {
        let s = self.settings();
        let cfg = s.ai.clone().ok_or_else(|| anyhow!("Connect an AI provider in Settings first."))?;
        let pre = ai::preset(&cfg.provider);
        let key = self.keys.get(&cfg.provider);
        if pre.needs_key && key.is_none() {
            return Err(anyhow!("Your {} key is missing. Add it in Settings.", pre.name));
        }
        let llm = LlmClient { base_url: cfg.base_url.clone(), key: key.clone(), model: cfg.model.clone(), window_min: pre.window_min };
        let layout = self.layout();
        let local_stt = || -> Option<Stt> {
            let tools = layout.tools(PathBuf::new(), PathBuf::new()).ok()?;
            let bin = tools.whisper?;
            let pref = deps::whisper_model_file(&cfg.whisper_size);
            let model = [pref, "ggml-small.en-q5_1.bin", "ggml-base.en-q5_1.bin"].iter().map(|m| layout.models.join(m)).find(|p| p.is_file())?;
            Some(Stt::Local { bin, model })
        };
        let stt = if cfg.stt == "local" {
            local_stt().ok_or_else(|| anyhow!("On-device Whisper isn't installed. Install it in Settings, or switch transcription to the cloud."))?
        } else {
            let sp = cfg.stt_provider.clone().unwrap_or_else(|| cfg.provider.clone());
            let spre = ai::preset(&sp);
            match (spre.stt_model, self.keys.get(&sp)) {
                (Some(model), Some(k)) => Stt::Cloud(CloudStt { base_url: spre.base_url.into(), key: k, model: model.into() }),
                _ => local_stt().ok_or_else(|| {
                    anyhow!("{} can pick clips but can't transcribe. Add a free Groq key for transcription, or install on-device Whisper in Settings.", pre.name)
                })?,
            }
        };
        Ok(JobCtx { stt, llm: Some(llm), settings: s.job.clone(), listen_budget_min: s.listen_budget_min })
    }

    pub fn queue_state(&self) -> QueueState {
        let q = self.queue.lock().unwrap();
        let ex = self.exports.lock().unwrap();
        QueueState {
            current: q.current.as_ref().map(|c| c.0.clone()),
            pending: q.pending.iter().cloned().collect(),
            exporting: ex.1.clone(),
            export_pending: ex.0.iter().cloned().collect(),
        }
    }
}

pub fn emit_queue(app: &AppHandle) {
    let st = app.state::<AppState>();
    let _ = app.emit("cd://queue", st.queue_state());
}

fn emitter(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |e: Event| {
        let _ = app.emit("cd://event", &e);
    })
}

/// One job at a time, in order.
pub fn spawn_job_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let st = app.state::<AppState>();
            let next = {
                let mut q = st.queue.lock().unwrap();
                if q.current.is_none() {
                    q.pending.pop_front().map(|id| {
                        let tok = CancellationToken::new();
                        q.current = Some((id.clone(), tok.clone()));
                        (id, tok)
                    })
                } else {
                    None
                }
            };
            let Some((id, tok)) = next else {
                st.queue_wake.notified().await;
                continue;
            };
            emit_queue(&app);
            let emit = emitter(&app);
            match (st.engine().await, st.job_ctx()) {
                (Ok(engine), Ok(ctx)) => {
                    let _ = engine.run(&id, &ctx, emit, &tok).await;
                }
                (Err(e), _) | (_, Err(e)) => {
                    emit(Event::Failed { project_id: id.clone(), error: e.to_string() });
                }
            }
            st.queue.lock().unwrap().current = None;
            emit_queue(&app);
        }
    });
}

#[derive(Clone, Serialize)]
struct ExportProgress {
    project_id: String,
    clip_id: String,
    progress: f64,
}

/// Final renders, one at a time.
pub fn spawn_export_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let st = app.state::<AppState>();
            let next = {
                let mut ex = st.exports.lock().unwrap();
                if ex.1.is_none() {
                    let n = ex.0.pop_front();
                    ex.1 = n.clone();
                    n
                } else {
                    None
                }
            };
            let Some((pid, cid)) = next else {
                st.export_wake.notified().await;
                continue;
            };
            emit_queue(&app);
            let s = st.settings();
            if let Ok(engine) = st.engine().await {
                let app2 = app.clone();
                let (p2, c2) = (pid.clone(), cid.clone());
                let mut last = 0.0;
                let r = engine
                    .export_clip(&pid, &cid, &PathBuf::from(&s.export_dir), s.job.remove_silence, &CancellationToken::new(), move |f| {
                        if f - last >= 0.02 || f >= 1.0 {
                            last = f;
                            let _ = app2.emit("cd://export", ExportProgress { project_id: p2.clone(), clip_id: c2.clone(), progress: f });
                        }
                    })
                    .await;
                match r {
                    Ok(clip) => {
                        let _ = app.emit("cd://event", Event::Clip { project_id: pid.clone(), clip });
                    }
                    Err(e) => {
                        let _ = app.emit("cd://event", Event::Notice { project_id: pid.clone(), message: format!("Export failed: {e}") });
                        if let Ok(p) = engine.store.load(&pid) {
                            if let Some(c) = p.clips.into_iter().find(|c| c.id == cid) {
                                let _ = app.emit("cd://event", Event::Clip { project_id: pid.clone(), clip: c });
                            }
                        }
                    }
                }
            }
            st.exports.lock().unwrap().1 = None;
            emit_queue(&app);
        }
    });
}

pub fn enqueue_export(app: &AppHandle, pid: &str, cid: &str) {
    let st = app.state::<AppState>();
    {
        let mut ex = st.exports.lock().unwrap();
        let item = (pid.to_string(), cid.to_string());
        if ex.1.as_ref() != Some(&item) && !ex.0.contains(&item) {
            ex.0.push_back(item);
        }
    }
    st.export_wake.notify_one();
    emit_queue(app);
}

/// Requeue exports for clips left "kept" or "exporting" when the app closed.
pub async fn resume_exports(app: &AppHandle) {
    let st = app.state::<AppState>();
    let Ok(engine) = st.engine().await else { return };
    for p in engine.store.list() {
        for c in p.clips {
            if matches!(c.status, ClipStatus::Kept | ClipStatus::Exporting) && c.export_path.is_none() && c.error.is_none() {
                enqueue_export(app, &p.id, &c.id);
            }
        }
    }
}
