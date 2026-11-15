//! Commands the front end can call.

use crate::state::{AppState, HitRect, QueueState, emit_queue, enqueue_export};
use cutdeck_engine::ai::{self, AiConfig, PRESETS, Preset};
use cutdeck_engine::captions::{CaptionStyle, STYLES};
use cutdeck_engine::deps::{self, DepsStatus};
use cutdeck_engine::source;
use cutdeck_engine::types::{ClipStatus, Clip, Layout, Project};
use cutdeck_engine::CancellationToken;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[derive(Serialize)]
pub struct AppInfo {
    settings: crate::settings::Settings,
    has_key: bool,
    deps: DepsStatus,
    presets: &'static [Preset],
    styles: &'static [CaptionStyle],
    platform: &'static str,
    version: &'static str,
    data_dir: String,
}

#[tauri::command]
pub fn app_info(st: State<AppState>) -> AppInfo {
    let s = st.settings();
    let has_key = s.ai.as_ref().map(|a| !ai::preset(&a.provider).needs_key || st.keys.get(&a.provider).is_some()).unwrap_or(false);
    AppInfo {
        settings: s,
        has_key,
        deps: st.layout().status(),
        presets: PRESETS,
        styles: STYLES,
        platform: if cfg!(windows) { "windows" } else { "macos" },
        version: env!("CARGO_PKG_VERSION"),
        data_dir: st.data_dir.to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub async fn install_deps(app: AppHandle) -> R<DepsStatus> {
    let st = app.state::<AppState>();
    if st.deps_busy.swap(true, Ordering::SeqCst) {
        return Err("Already installing".into());
    }
    let app2 = app.clone();
    let r = deps::install(&st.layout(), &CancellationToken::new(), &move |p| {
        let _ = app2.emit("cd://deps", &p);
    })
    .await;
    st.deps_busy.store(false, Ordering::SeqCst);
    st.reset_engine().await;
    r.map_err(|err| format!("{err:#}"))?;
    Ok(st.layout().status())
}

#[tauri::command]
pub async fn install_whisper(app: AppHandle, size: String) -> R<DepsStatus> {
    let st = app.state::<AppState>();
    let app2 = app.clone();
    deps::install_whisper(&st.layout(), &size, &CancellationToken::new(), &move |p| {
        let _ = app2.emit("cd://deps", &p);
    })
    .await
    .map_err(|err| format!("{err:#}"))?;
    st.reset_engine().await;
    Ok(st.layout().status())
}

#[derive(Serialize)]
pub struct AiTest {
    models: Vec<String>,
    chosen: Option<String>,
}

#[tauri::command]
pub async fn test_ai(st: State<'_, AppState>, provider: String, base_url: Option<String>, key: Option<String>) -> R<AiTest> {
    let pre = ai::preset(&provider);
    let base = base_url.filter(|b| !b.trim().is_empty()).unwrap_or_else(|| pre.base_url.to_string());
    let key = key.filter(|k| !k.trim().is_empty()).map(|k| k.trim().to_string()).or_else(|| st.keys.get(&provider));
    if pre.needs_key && key.is_none() {
        return Err(format!("Paste your {} key first.", pre.name));
    }
    let models = ai::list_models(&base, &key).await.map_err(e)?;
    let chosen = ai::choose_model(&models, pre.prefs);
    if chosen.is_none() {
        return Err(if provider == "local" { "The local server has no models yet. Pull one first, e.g. `ollama pull qwen3:8b`.".into() } else { "No usable model found on this key.".into() });
    }
    Ok(AiTest { models, chosen })
}

#[tauri::command]
pub fn save_ai(st: State<AppState>, config: AiConfig, key: Option<String>, stt_key: Option<String>) -> R<()> {
    if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
        st.keys.set(&config.provider, k.trim()).map_err(e)?;
    }
    if let (Some(sp), Some(k)) = (config.stt_provider.as_ref(), stt_key.filter(|k| !k.trim().is_empty())) {
        st.keys.set(sp, k.trim()).map_err(e)?;
    }
    let mut s = st.settings();
    s.ai = Some(config);
    st.save_settings(s).map_err(e)
}

#[tauri::command]
pub fn forget_key(st: State<AppState>, provider: String) {
    st.keys.delete(&provider);
}

#[tauri::command]
pub fn save_settings(app: AppHandle, st: State<AppState>, settings: crate::settings::Settings) -> R<()> {
    let old = st.settings();
    let mut s = settings;
    s.ai = s.ai.or(old.ai);
    s.tray_corner = old.tray_corner.clone();
    if s.launch_at_login != old.launch_at_login {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let _ = if s.launch_at_login { al.enable() } else { al.disable() };
    }
    st.save_settings(s).map_err(e)
}

#[tauri::command]
pub fn finish_onboarding(app: AppHandle, st: State<AppState>) -> R<()> {
    let mut s = st.settings();
    s.onboarded = true;
    st.save_settings(s).map_err(e)?;
    crate::tray::show(&app);
    Ok(())
}

#[tauri::command]
pub async fn start_job(app: AppHandle, input: String) -> R<String> {
    let st = app.state::<AppState>();
    let input = input.trim().trim_matches('"').to_string();
    if input.is_empty() {
        return Err("Paste a link or drop a video.".into());
    }
    if !source::is_url(&input) {
        let ext = std::path::Path::new(&input).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !["mp4", "mov", "mkv", "webm", "m4v", "avi", "flv", "ts"].contains(&ext.as_str()) {
            return Err("That doesn't look like a video file.".into());
        }
    }
    st.job_ctx().map_err(e)?;
    let engine = st.engine().await.map_err(e)?;
    let id = engine.open(&input).await.map_err(e)?;
    let _ = engine.store.update(&id, |p| p.error = None).await;
    {
        let mut q = st.queue.lock().unwrap();
        let running = q.current.as_ref().map(|c| c.0 == id).unwrap_or(false);
        if !running && !q.pending.contains(&id) {
            q.pending.push_back(id.clone());
        }
    }
    st.queue_wake.notify_one();
    if let Ok(p) = engine.store.load(&id) {
        let _ = app.emit("cd://event", cutdeck_engine::Event::Project { project: p });
    }
    emit_queue(&app);
    Ok(id)
}

#[tauri::command]
pub fn cancel_job(app: AppHandle, st: State<AppState>, project_id: String) {
    {
        let mut q = st.queue.lock().unwrap();
        q.pending.retain(|p| p != &project_id);
        if let Some((id, tok)) = &q.current {
            if id == &project_id {
                tok.cancel();
            }
        }
    }
    emit_queue(&app);
}

#[tauri::command]
pub fn queue_state(st: State<AppState>) -> QueueState {
    st.queue_state()
}

#[tauri::command]
pub async fn list_projects(st: State<'_, AppState>) -> R<Vec<Project>> {
    match st.engine().await {
        Ok(engine) => Ok(engine.store.list()),
        Err(_) => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn delete_project(app: AppHandle, project_id: String) -> R<()> {
    let st = app.state::<AppState>();
    cancel_job(app.clone(), app.state::<AppState>(), project_id.clone());
    let engine = st.engine().await.map_err(e)?;
    engine.store.delete(&project_id).await.map_err(e)
}

#[tauri::command]
pub async fn keep_clip(app: AppHandle, project_id: String, clip_id: String) -> R<Clip> {
    let st = app.state::<AppState>();
    let engine = st.engine().await.map_err(e)?;
    let c = engine.set_status(&project_id, &clip_id, ClipStatus::Kept).await.map_err(e)?;
    enqueue_export(&app, &project_id, &clip_id);
    Ok(c)
}

#[tauri::command]
pub async fn discard_clip(st: State<'_, AppState>, project_id: String, clip_id: String) -> R<Clip> {
    let engine = st.engine().await.map_err(e)?;
    engine.set_status(&project_id, &clip_id, ClipStatus::Discarded).await.map_err(e)
}

#[tauri::command]
pub async fn restore_clip(st: State<'_, AppState>, project_id: String, clip_id: String) -> R<Clip> {
    let engine = st.engine().await.map_err(e)?;
    engine.set_status(&project_id, &clip_id, ClipStatus::Ready).await.map_err(e)
}

#[derive(Deserialize)]
pub struct ClipPatch {
    layout: Option<Layout>,
    caption_style: Option<String>,
    hook_banner: Option<bool>,
    hook: Option<String>,
    title: Option<String>,
    /// [first, last] word index; null resets to automatic.
    trim: Option<Option<(usize, usize)>>,
    deleted: Option<Vec<usize>>,
    crop_x: Option<Option<f32>>,
}

#[tauri::command]
pub async fn edit_clip(app: AppHandle, project_id: String, clip_id: String, patch: ClipPatch) -> R<Clip> {
    let st = app.state::<AppState>();
    let engine = st.engine().await.map_err(e)?;
    let rs = st.settings().job.remove_silence;
    let c = engine
        .edit_clip(
            &project_id,
            &clip_id,
            rs,
            |c| {
                if let Some(l) = patch.layout {
                    c.layout = l;
                    c.edits.crop_x = None;
                }
                if let Some(s) = patch.caption_style {
                    c.caption_style = s;
                }
                if let Some(b) = patch.hook_banner {
                    c.hook_banner = b;
                }
                if let Some(h) = patch.hook {
                    c.pick.hook = h;
                }
                if let Some(t) = patch.title {
                    c.pick.title = t;
                }
                if let Some(t) = patch.trim {
                    c.edits.trim = t;
                }
                if let Some(d) = patch.deleted {
                    c.edits.deleted = d;
                }
                if let Some(x) = patch.crop_x {
                    c.edits.crop_x = x;
                }
            },
            &CancellationToken::new(),
        )
        .await
        .map_err(|err| format!("{err:#}"))?;
    let _ = app.emit("cd://event", cutdeck_engine::Event::Clip { project_id: project_id.clone(), clip: c.clone() });
    Ok(c)
}

#[tauri::command]
pub fn export_clip(app: AppHandle, project_id: String, clip_id: String) {
    enqueue_export(&app, &project_id, &clip_id);
}

#[tauri::command]
pub fn set_hit_rects(st: State<AppState>, rects: Vec<HitRect>, drop_zone: Option<HitRect>) {
    *st.hit.lock().unwrap() = rects;
    *st.drop_zone.lock().unwrap() = drop_zone;
}

#[tauri::command]
pub fn focus_tray(app: AppHandle) {
    if let Some(w) = app.get_webview_window("tray") {
        let _ = w.set_ignore_cursor_events(false);
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn drag_tray(app: AppHandle) {
    crate::tray::drag(&app);
}

#[tauri::command]
pub fn set_tray_corner(app: AppHandle, corner: String) -> R<()> {
    if !["br", "bl", "tr", "tl"].contains(&corner.as_str()) {
        return Err("unknown corner".into());
    }
    crate::tray::set_corner(&app, &corner);
    Ok(())
}

#[tauri::command]
pub fn open_studio(app: AppHandle, view: Option<String>) {
    crate::show_studio(&app, view);
}

#[tauri::command]
pub fn reveal(path: String) -> R<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let p = path.replace('/', "\\");
        std::process::Command::new("explorer").raw_arg(format!("/select,\"{p}\"")).spawn().map_err(e)?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("open").arg("-R").arg(&path).spawn().map_err(e)?;
    }
    Ok(())
}
#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
