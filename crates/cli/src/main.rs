//! `cutdeck` command line: runs the same engine as the app.
//!
//! cutdeck deps                          install FFmpeg, yt-dlp, Deno into the app data folder
//! cutdeck whisper [base|small]          install on-device Whisper
//! cutdeck run <file|url> [options]      find and render clips
//!   --out <dir>        export finished clips here (default: preview only)
//!   --clips <n>        number of clips (default 6)
//!   --style <id>       caption style: beast, hype, bold, neon, clean
//!   --content <kind>   auto, gaming, podcast, vlog
//!   --layout <l>       auto, face, split, fit, center
//!   --local            transcribe with on-device Whisper
//!
//! AI settings come from the environment: CUTDECK_PROVIDER (groq, gemini, xai,
//! openai, openrouter, local), CUTDECK_KEY, optional CUTDECK_MODEL, CUTDECK_BASE_URL.

use anyhow::{Result, anyhow, bail};
use cutdeck_engine::ai::{self, CloudStt, LlmClient};
use cutdeck_engine::deps::{self, Layout as DepLayout};
use cutdeck_engine::transcribe::Stt;
use cutdeck_engine::types::{JobSettings, Layout};
use cutdeck_engine::{CancellationToken, Engine, Event, JobCtx};
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")).unwrap_or_else(|| PathBuf::from("."))
    };
    base.join("com.mohanbuilds.cutdeck")
}

fn resources() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources")
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).with_writer(std::io::stderr).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let layout = DepLayout::new(&data_dir());
    let cancel = CancellationToken::new();
    match args.first().map(String::as_str) {
        Some("deps") => {
            deps::install(&layout, &cancel, &|p| eprintln!("{} {:>5.1}%{}", p.label, if p.total > 0 { p.done as f64 * 100.0 / p.total as f64 } else { 0.0 }, if p.finished { " done" } else { "" })).await?;
            println!("{:?}", layout.status());
        }
        Some("whisper") => {
            let size = args.get(1).cloned().unwrap_or_else(|| "base".into());
            deps::install_whisper(&layout, &size, &cancel, &|p| eprintln!("{} {}/{}", p.label, p.done, p.total)).await?;
            println!("{:?}", layout.status());
        }
        Some("rerender") => {
            // Re-render every clip of a project with the current engine (layout reset to Auto).
            let id = args.get(1).ok_or_else(|| anyhow!("usage: cutdeck rerender <project id> [--layout l] [--style id]"))?.clone();
            let tools = layout.tools(resources().join("fonts"), resources().join("models/seeta_fd_frontal_v1.0.bin"))?;
            let engine = Engine::new(tools, data_dir().join("projects"));
            let lay: Layout = arg(&args, "--layout").and_then(|l| serde_json::from_value(serde_json::Value::String(l)).ok()).unwrap_or(Layout::Auto);
            let style = arg(&args, "--style");
            for c in engine.store.load(&id)?.clips {
                let s = style.clone();
                let t = std::time::Instant::now();
                let done = engine
                    .edit_clip(&id, &c.id, true, |c| {
                        c.layout = lay;
                        if let Some(s) = s {
                            c.caption_style = s;
                        }
                    }, &cancel)
                    .await?;
                eprintln!("{} -> {:?} {:.1}s in {:.1}s", done.id, done.resolved_layout, done.duration, t.elapsed().as_secs_f64());
            }
        }
        Some("run") => {
            let input = args.get(1).ok_or_else(|| anyhow!("usage: cutdeck run <file|url>"))?.clone();
            let tools = layout.tools(resources().join("fonts"), resources().join("models/seeta_fd_frontal_v1.0.bin"))?;
            let engine = Engine::new(tools.clone(), data_dir().join("projects"));
            let provider = std::env::var("CUTDECK_PROVIDER").unwrap_or_else(|_| "groq".into());
            let key = std::env::var("CUTDECK_KEY").ok();
            let pre = ai::preset(&provider);
            let base = std::env::var("CUTDECK_BASE_URL").unwrap_or_else(|_| pre.base_url.to_string());
            let local = args.iter().any(|a| a == "--local");
            let stt = if local || key.is_none() || pre.stt_model.is_none() {
                let bin = tools.whisper.clone().ok_or_else(|| anyhow!("no transcription available: set CUTDECK_KEY for Groq/OpenAI or run `cutdeck whisper`"))?;
                let model = ["ggml-small.en-q5_1.bin", "ggml-base.en-q5_1.bin"].iter().map(|m| tools.models_dir.join(m)).find(|p| p.is_file()).ok_or_else(|| anyhow!("no Whisper model; run `cutdeck whisper`"))?;
                Stt::Local { bin, model }
            } else {
                Stt::Cloud(CloudStt { base_url: base.clone(), key: key.clone().unwrap(), model: pre.stt_model.unwrap().into() })
            };
            let llm = if key.is_some() || !pre.needs_key {
                let model = match std::env::var("CUTDECK_MODEL") {
                    Ok(m) => m,
                    Err(_) => {
                        let ms = ai::list_models(&base, &key).await?;
                        ai::choose_model(&ms, pre.prefs).ok_or_else(|| anyhow!("no usable model"))?
                    }
                };
                eprintln!("picking with {provider} / {model}");
                Some(LlmClient { base_url: base, key, model, window_min: pre.window_min })
            } else {
                eprintln!("no AI key: using the built-in picker");
                None
            };
            let mut settings = JobSettings { max_clips: arg(&args, "--clips").and_then(|v| v.parse().ok()).unwrap_or(6), ..Default::default() };
            if let Some(s) = arg(&args, "--style") {
                settings.caption_style = s;
            }
            if let Some(c) = arg(&args, "--content") {
                settings.content = c;
            }
            if let Some(l) = arg(&args, "--layout") {
                settings.layout = serde_json::from_value(serde_json::Value::String(l)).unwrap_or(Layout::Auto);
            }
            let ctx = JobCtx { stt, llm, settings, listen_budget_min: 60.0 };
            let id = engine.open(&input).await?;
            println!("project {id} in {}", engine.store.dir(&id).display());
            let emit: cutdeck_engine::Emit = Arc::new(|e: Event| match e {
                Event::Progress { label, progress, detail, .. } => eprintln!("[{:>3.0}%] {label}: {detail}", progress * 100.0),
                Event::Clip { clip, .. } => eprintln!(
                    "clip {} ready: {:.1}s {:?} score {:.0} \"{}\" -> {}",
                    clip.id,
                    clip.duration,
                    clip.resolved_layout,
                    clip.pick.score,
                    clip.pick.title,
                    clip.preview.unwrap_or_default()
                ),
                Event::Notice { message, .. } => eprintln!("note: {message}"),
                Event::Failed { error, .. } => eprintln!("FAILED: {error}"),
                _ => {}
            });
            let t0 = std::time::Instant::now();
            engine.run(&id, &ctx, emit, &cancel).await?;
            eprintln!("job finished in {:.1}s", t0.elapsed().as_secs_f64());
            if let Some(out) = arg(&args, "--out") {
                let p = engine.store.load(&id)?;
                for c in p.clips {
                    let t = std::time::Instant::now();
                    let done = engine.export_clip(&id, &c.id, &PathBuf::from(&out), true, &cancel, |_| {}).await?;
                    eprintln!("exported {} in {:.1}s -> {}", c.id, t.elapsed().as_secs_f64(), done.export_path.unwrap_or_default());
                }
            }
        }
        _ => bail!("usage: cutdeck deps | whisper [base|small] | run <file|url> [--out dir] [--clips n] [--style id] [--content kind] [--layout l] [--local]"),
    }
    Ok(())
}
