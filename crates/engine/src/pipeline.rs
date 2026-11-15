//! The job: input (file or link) → transcript → picks → rendered clip previews.
//! Each stage saves its output so a restarted job resumes where it stopped.

use crate::ai::LlmClient;
use crate::captions::{self, CaptionInput};
use crate::deps::Tools;
use crate::edit::{EditList, EditParams};
use crate::faces::{self, FaceData};
use crate::frame;
use crate::media;
use crate::pick::{self, Chat, Energy, HOP};
use crate::project::{Store, now, project_id, read_json, write_json};
use crate::render::{RenderSpec, render};
use crate::source;
use crate::transcribe::{Stt, transcribe};
use crate::types::*;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

pub struct Engine {
    pub tools: Tools,
    pub store: Store,
    encoder: OnceCell<String>,
}

pub struct JobCtx {
    pub stt: Stt,
    pub llm: Option<LlmClient>,
    pub settings: JobSettings,
    /// Max minutes of audio to transcribe for very long videos.
    pub listen_budget_min: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Progress { project_id: String, stage: Stage, label: String, progress: f64, detail: String },
    Project { project: Project },
    Clip { project_id: String, clip: Clip },
    Notice { project_id: String, message: String },
    Done { project_id: String },
    Failed { project_id: String, error: String },
}

pub type Emit = Arc<dyn Fn(Event) + Send + Sync>;

fn progress(emit: &Emit, id: &str, stage: Stage, p: f64, detail: impl Into<String>) {
    emit(Event::Progress { project_id: id.into(), stage, label: stage.label().into(), progress: p.clamp(0.0, 1.0), detail: detail.into() });
}

fn out_fps(src: f64) -> f64 {
    if src >= 47.0 { 60.0 } else { 30.0 }
}

impl Engine {
    pub fn new(tools: Tools, projects_root: PathBuf) -> Self {
        Self { tools, store: Store::new(projects_root), encoder: OnceCell::new() }
    }

    pub async fn encoder(&self) -> String {
        self.encoder.get_or_init(|| media::detect_encoder(&self.tools)).await.clone()
    }

    fn ensure_fonts(&self, pdir: &Path) -> Result<()> {
        let dst = pdir.join("fonts");
        if dst.is_dir() {
            return Ok(());
        }
        std::fs::create_dir_all(&dst)?;
        for e in std::fs::read_dir(&self.tools.fonts_dir).context("fonts folder missing")?.flatten() {
            std::fs::copy(e.path(), dst.join(e.file_name()))?;
        }
        Ok(())
    }

    /// Create (or reopen) the project for an input and return its id.
    pub async fn open(&self, input: &str) -> Result<String> {
        let input = if source::is_url(input) { source::normalize_url(input) } else { input.trim().trim_matches('"').to_string() };
        if !source::is_url(&input) && !Path::new(&input).is_file() {
            bail!("File not found: {input}");
        }
        let id = project_id(&input);
        if self.store.load(&id).is_err() {
            let p = Project { id: id.clone(), created: now(), meta: None, clips: vec![], stage: "queued".into(), done: false, error: None };
            self.store.save_new(&p).await?;
            write_json(&self.store.dir(&id).join("input.json"), &input)?;
        }
        Ok(id)
    }

    pub async fn run(&self, id: &str, ctx: &JobCtx, emit: Emit, cancel: &CancellationToken) -> Result<()> {
        let r = self.run_inner(id, ctx, &emit, cancel).await;
        match &r {
            Ok(()) => {
                let (_, p) = self.store.update(id, |p| {
                    p.done = true;
                    p.stage = "done".into();
                    p.error = None;
                })
                .await?;
                emit(Event::Project { project: p });
                emit(Event::Done { project_id: id.into() });
            }
            Err(e) => {
                let msg = if e.to_string() == "cancelled" { "Cancelled".to_string() } else { format!("{e:#}") };
                let _ = self
                    .store
                    .update(id, |p| {
                        p.error = Some(msg.clone());
                    })
                    .await;
                emit(Event::Failed { project_id: id.into(), error: msg });
            }
        }
        r
    }

    async fn run_inner(&self, id: &str, ctx: &JobCtx, emit: &Emit, cancel: &CancellationToken) -> Result<()> {
        let pdir = self.store.dir(id);
        let input: String = read_json(&pdir.join("input.json"))?;
        let is_url = source::is_url(&input);
        self.ensure_fonts(&pdir)?;
        let set_stage = |s: Stage| {
            let key = s.key().to_string();
            async move {
                let _ = self.store.update(id, |p| p.stage = key).await;
            }
        };

        // 1. Fetch.
        set_stage(Stage::Fetching).await;
        progress(emit, id, Stage::Fetching, 0.0, if is_url { "Reading the link" } else { "Opening the file" });
        let meta_path = pdir.join("source.json");
        let meta: SourceMeta = match read_json(&meta_path) {
            Ok(m) => m,
            Err(_) => {
                let m = if is_url {
                    let i = source::info(&self.tools, &input, cancel).await?;
                    SourceMeta {
                        input: input.clone(),
                        is_url: true,
                        title: i.title.unwrap_or_else(|| "Video".into()),
                        duration: i.duration.unwrap_or(0.0),
                        width: i.width.unwrap_or(1920),
                        height: i.height.unwrap_or(1080),
                        fps: i.fps.unwrap_or(30.0),
                        uploader: i.uploader.or(i.channel),
                        thumbnail: i.thumbnail,
                        was_live: i.live_status.as_deref() == Some("was_live") || i.subtitles.get("live_chat").is_some(),
                    }
                } else {
                    let mi = media::probe(&self.tools, Path::new(&input), cancel).await?;
                    if mi.duration < 20.0 {
                        bail!("This video is shorter than 20 seconds. Cutdeck needs a longer video to find moments in.");
                    }
                    SourceMeta {
                        input: input.clone(),
                        is_url: false,
                        title: Path::new(&input).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Video".into()),
                        duration: mi.duration,
                        width: mi.width,
                        height: mi.height,
                        fps: mi.fps,
                        uploader: None,
                        thumbnail: None,
                        was_live: false,
                    }
                };
                write_json(&meta_path, &m)?;
                m
            }
        };
        let (_, p) = self.store.update(id, |p| p.meta = Some(meta.clone())).await?;
        emit(Event::Project { project: p });

        // 2. Audio.
        let wav = pdir.join("audio.wav");
        if !wav.is_file() {
            set_stage(Stage::Audio).await;
            let src_audio: PathBuf = if is_url {
                progress(emit, id, Stage::Fetching, 0.05, "Downloading the audio");
                let chat_task = if meta.was_live && input.contains("youtube.com") {
                    let tools = self.tools.clone();
                    let url = input.clone();
                    let dir = pdir.clone();
                    Some(tokio::spawn(async move { source::chat_times(&tools, &url, &dir, 120).await }))
                } else {
                    None
                };
                let a = source::download_audio(&self.tools, &input, &pdir, cancel, |f| progress(emit, id, Stage::Fetching, 0.05 + f * 0.9, "Downloading the audio")).await?;
                if let Some(t) = chat_task {
                    progress(emit, id, Stage::Fetching, 0.97, "Reading the chat replay");
                    let times = t.await.unwrap_or_default();
                    write_json(&pdir.join("chat.json"), &times)?;
                }
                a
            } else {
                PathBuf::from(&input)
            };
            progress(emit, id, Stage::Audio, 0.0, "Reading the audio");
            media::extract_wav(&self.tools, &src_audio, &wav, meta.duration, cancel, |f| progress(emit, id, Stage::Audio, f, "Reading the audio")).await?;
            if is_url {
                let _ = std::fs::remove_file(&src_audio);
            }
        }
        let energy_path = pdir.join("energy.json");
        let curve: Vec<f32> = match read_json(&energy_path) {
            Ok(c) => c,
            Err(_) => {
                let w = wav.clone();
                let c = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> { Ok(media::loudness_curve(&media::read_wav_samples(&w)?, 16000, HOP)) }).await??;
                write_json(&energy_path, &c)?;
                c
            }
        };
        let energy = Energy::new(curve);
        let duration = if meta.duration > 0.0 { meta.duration } else { energy.duration() };
        let chat = Chat::new(read_json(&pdir.join("chat.json")).unwrap_or_default(), duration);

        // 3. Listen.
        let tpath = pdir.join("transcript.json");
        let words: Vec<Word> = match read_json(&tpath) {
            Ok(w) => w,
            Err(_) => {
                set_stage(Stage::Listening).await;
                let regions = pick::choose_regions(&energy, &chat, duration, ctx.listen_budget_min * 60.0);
                let mins = regions.iter().map(|r| r.1 - r.0).sum::<f64>() / 60.0;
                let detail = if regions.len() > 1 { format!("Listening to the {mins:.0} liveliest minutes") } else { "Listening".to_string() };
                progress(emit, id, Stage::Listening, 0.0, detail.clone());
                let e2 = emit.clone();
                let id2 = id.to_string();
                let d2 = detail.clone();
                let on = move |f: f64| progress(&e2, &id2, Stage::Listening, f, d2.clone());
                let w = transcribe(&ctx.stt, &self.tools, &wav, &regions, &pdir, cancel, &on).await?;
                write_json(&tpath, &w)?;
                w
            }
        };

        // 4. Pick.
        let picks_path = pdir.join("picks.json");
        let picks: Vec<Pick> = match read_json(&picks_path) {
            Ok(p) => p,
            Err(_) => {
                set_stage(Stage::Picking).await;
                progress(emit, id, Stage::Picking, 0.0, "Finding the moments");
                let sents = pick::sentences(&words, &energy, &chat);
                let mut picks = match &ctx.llm {
                    Some(llm) => {
                        let e2 = emit.clone();
                        let id2 = id.to_string();
                        let on = move |f: f64| progress(&e2, &id2, Stage::Picking, f * 0.95, "Finding the moments");
                        match pick::llm_picks(llm, &sents, &energy, &chat, &ctx.settings, &meta.title, cancel, &on).await {
                            Ok(p) if !p.is_empty() => p,
                            Ok(_) => pick::heuristic_picks(&sents, &energy, &chat, &ctx.settings),
                            Err(e) if e.to_string() == "cancelled" => return Err(e),
                            Err(e) => {
                                emit(Event::Notice { project_id: id.into(), message: format!("The AI couldn't pick moments ({e}). Using Cutdeck's built-in picker instead.") });
                                pick::heuristic_picks(&sents, &energy, &chat, &ctx.settings)
                            }
                        }
                    }
                    None => pick::heuristic_picks(&sents, &energy, &chat, &ctx.settings),
                };
                picks.truncate(ctx.settings.max_clips);
                if picks.is_empty() {
                    bail!("Cutdeck didn't find any moments that fit the clip length.");
                }
                write_json(&picks_path, &picks)?;
                picks
            }
        };

        // 5. Cut.
        set_stage(Stage::Cutting).await;
        let n = picks.len();
        for (i, pk) in picks.iter().enumerate() {
            let cid = format!("c{i:02}");
            let existing = self.store.load(id)?.clips.into_iter().find(|c| c.id == cid);
            if let Some(c) = &existing {
                if c.preview.is_some() && c.status != ClipStatus::Pending && c.status != ClipStatus::Failed {
                    continue;
                }
            }
            let base = i as f64 / n as f64;
            let span = 1.0 / n as f64;
            let detail = format!("Cutting clip {} of {n}", i + 1);
            progress(emit, id, Stage::Cutting, base, detail.clone());
            match self.cut_clip(id, &pdir, &meta, &input, i, pk, &words, ctx, emit, cancel, base, span, &detail).await {
                Ok(clip) => {
                    emit(Event::Clip { project_id: id.into(), clip });
                }
                Err(e) if e.to_string() == "cancelled" => return Err(e),
                Err(e) => {
                    tracing::warn!("clip {cid} failed: {e:#}");
                    emit(Event::Notice { project_id: id.into(), message: format!("Clip {} failed: {e}", i + 1) });
                }
            }
        }
        progress(emit, id, Stage::Done, 1.0, "Done");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn cut_clip(
        &self,
        id: &str,
        pdir: &Path,
        meta: &SourceMeta,
        input: &str,
        index: usize,
        pk: &Pick,
        words: &[Word],
        ctx: &JobCtx,
        emit: &Emit,
        cancel: &CancellationToken,
        base: f64,
        span: f64,
        detail: &str,
    ) -> Result<Clip> {
        let cid = format!("c{index:02}");
        let cdir = pdir.join("clips").join(&cid);
        std::fs::create_dir_all(&cdir)?;
        let ss = (pk.start - 2.0).max(0.0);
        let se = (pk.end + 2.0).min(if meta.duration > 0.0 { meta.duration } else { pk.end + 2.0 });
        let fps = out_fps(meta.fps);
        let section = cdir.join("section.mp4");
        let rep = |f: f64| progress(emit, id, Stage::Cutting, base + span * f, detail.to_string());
        if !section.is_file() {
            if meta.is_url {
                let stem = cdir.join("raw");
                let raw = source::download_section(&self.tools, input, ss, se, &stem, cancel, |f| rep(f * 0.45)).await?;
                let mi = media::probe(&self.tools, &raw, cancel).await?;
                media::extract_section(&self.tools, &raw, 0.0, se - ss, fps, mi.has_audio, &section, cancel, |f| rep(0.45 + f * 0.15)).await?;
                let _ = std::fs::remove_file(raw);
            } else {
                let mi = media::probe(&self.tools, Path::new(input), cancel).await?;
                media::extract_section(&self.tools, Path::new(input), ss, se - ss, fps, mi.has_audio, &section, cancel, |f| rep(f * 0.6)).await?;
            }
        }
        let si = media::probe(&self.tools, &section, cancel).await?;
        let faces_path = cdir.join("faces.json");
        if !faces_path.is_file() {
            rep(0.62);
            let fd = faces::detect(&self.tools, &section, si.width, si.height, cancel).await?;
            write_json(&faces_path, &fd)?;
        }
        let local: Vec<Word> = words.iter().filter(|w| w.s >= ss && w.e <= se).map(|w| Word { w: w.w.clone(), s: w.s - ss, e: w.e - ss }).collect();
        let a = local.iter().position(|w| w.s >= pk.start - ss - 0.3).unwrap_or(0);
        let b = local.iter().rposition(|w| w.e <= pk.end - ss + 0.3).unwrap_or(local.len().saturating_sub(1)).max(a);
        let mut clip = Clip {
            id: cid.clone(),
            index,
            pick: pk.clone(),
            section_start: ss,
            section_end: se,
            src_w: si.width,
            src_h: si.height,
            fps,
            words: local,
            auto_range: (a, b),
            layout: ctx.settings.layout,
            resolved_layout: Layout::Auto,
            caption_style: ctx.settings.caption_style.clone(),
            hook_banner: ctx.settings.hook_banner,
            edits: ClipEdits::default(),
            status: ClipStatus::Pending,
            duration: 0.0,
            preview: None,
            thumb: None,
            export_path: None,
            error: None,
            rev: 0,
        };
        self.render_clip(id, &mut clip, None, ctx.settings.remove_silence, cancel, |f| rep(0.7 + f * 0.28)).await?;
        clip.status = ClipStatus::Ready;
        let c2 = clip.clone();
        self.store
            .update(id, move |p| {
                p.clips.retain(|c| c.id != c2.id);
                p.clips.push(c2);
                p.clips.sort_by_key(|c| c.index);
            })
            .await?;
        Ok(clip)
    }

    /// Render a clip's preview (when `export_to` is None) or its final export.
    pub async fn render_clip(
        &self,
        id: &str,
        clip: &mut Clip,
        export_to: Option<&Path>,
        remove_silence: bool,
        cancel: &CancellationToken,
        on: impl FnMut(f64),
    ) -> Result<()> {
        let pdir = self.store.dir(id);
        self.ensure_fonts(&pdir)?;
        let cdir = pdir.join("clips").join(&clip.id);
        let section = cdir.join("section.mp4");
        if !section.is_file() {
            bail!("This clip's source is missing. Re-run the video.");
        }
        let fd: Option<FaceData> = read_json(&cdir.join("faces.json")).ok();
        let curve: Vec<f32> = read_json(&pdir.join("energy.json")).unwrap_or_default();
        let energy = Energy::new(curve);
        let resolved = frame::resolve(clip.layout, fd.as_ref(), clip.src_w, clip.src_h);
        clip.resolved_layout = resolved;
        let (start, end) = match clip.edits.trim {
            Some((a, b)) if a <= b && b < clip.words.len() => ((clip.words[a].s - 0.15).max(0.0), (clip.words[b].e + 0.4).min(clip.section_end - clip.section_start)),
            _ => ((clip.pick.start - clip.section_start).max(0.0), (clip.pick.end - clip.section_start).min(clip.section_end - clip.section_start)),
        };
        let off = clip.section_start;
        let quiet = move |s: f64, e: f64| energy.z(s + off, e + off) < -0.6;
        let edit = EditList::build(&EditParams { words: &clip.words, start, end, deleted: &clip.edits.deleted, remove_silence, fps: clip.fps, quiet: &quiet });
        let framing = frame::plan(resolved, fd.as_ref(), clip.src_w, clip.src_h, clip.fps, clip.section_end - clip.section_start, clip.edits.crop_x);
        let words = edit.retime(&clip.words, &clip.edits.deleted);
        let hook = if clip.hook_banner { Some(clip.pick.hook.as_str()) } else { None };
        let ass = captions::build_ass(&CaptionInput { words: &words, style: captions::style(&clip.caption_style), layout: framing.layout, hook, duration: edit.duration() });
        std::fs::write(cdir.join("captions.ass"), ass)?;
        let preview = export_to.is_none();
        let out = match export_to {
            Some(p) => p.to_path_buf(),
            None => cdir.join("preview.mp4"),
        };
        let encoder = if preview { "libx264".to_string() } else { self.encoder().await };
        let spec = RenderSpec {
            tools: &self.tools,
            section: &section,
            src_w: clip.src_w,
            src_h: clip.src_h,
            fps: clip.fps,
            framing: &framing,
            edit: &edit,
            ass_name: "captions.ass",
            fonts_rel: "../../fonts",
            cwd: &cdir,
            out: &out,
            preview,
            encoder: &encoder,
        };
        let r = render(&spec, cancel, on).await;
        if let Err(e) = r {
            if !preview && encoder != "libx264" && e.to_string() != "cancelled" {
                // Hardware encoders can fail on odd drivers; retry in software.
                tracing::warn!("{encoder} failed, retrying with libx264: {e}");
                let spec2 = RenderSpec { encoder: "libx264", ..spec };
                render(&spec2, cancel, |_| {}).await?;
            } else {
                return Err(e);
            }
        }
        clip.duration = edit.duration();
        if preview {
            let thumb = cdir.join("thumb.jpg");
            media::thumbnail(&self.tools, &out, (clip.duration * 0.3).min(4.0), &thumb, cancel).await?;
            clip.preview = Some(out.to_string_lossy().to_string());
            clip.thumb = Some(thumb.to_string_lossy().to_string());
            clip.rev += 1;
        }
        Ok(())
    }

    /// Apply a change to a clip and re-render its preview.
    pub async fn edit_clip(&self, id: &str, clip_id: &str, remove_silence: bool, change: impl FnOnce(&mut Clip), cancel: &CancellationToken) -> Result<Clip> {
        let mut clip = self.store.load(id)?.clips.into_iter().find(|c| c.id == clip_id).ok_or_else(|| anyhow!("clip not found"))?;
        change(&mut clip);
        self.render_clip(id, &mut clip, None, remove_silence, cancel, |_| {}).await?;
        let c2 = clip.clone();
        self.store
            .update(id, move |p| {
                if let Some(c) = p.clips.iter_mut().find(|c| c.id == c2.id) {
                    let status = c.status;
                    let export = c.export_path.clone();
                    *c = c2;
                    // A changed clip needs a fresh export.
                    c.status = if status == ClipStatus::Exported { ClipStatus::Kept } else { status };
                    c.export_path = if status == ClipStatus::Exported { None } else { export };
                }
            })
            .await?;
        Ok(clip)
    }

    pub async fn set_status(&self, id: &str, clip_id: &str, status: ClipStatus) -> Result<Clip> {
        let (c, _) = self
            .store
            .update(id, |p| {
                p.clips.iter_mut().find(|c| c.id == clip_id).map(|c| {
                    c.status = status;
                    c.clone()
                })
            })
            .await?;
        c.ok_or_else(|| anyhow!("clip not found"))
    }

    /// Render the final 1080x1920 file into `out_dir`.
    pub async fn export_clip(&self, id: &str, clip_id: &str, out_dir: &Path, remove_silence: bool, cancel: &CancellationToken, on: impl FnMut(f64)) -> Result<Clip> {
        let p = self.store.load(id)?;
        let mut clip = p.clips.iter().find(|c| c.id == clip_id).cloned().ok_or_else(|| anyhow!("clip not found"))?;
        let title = p.meta.as_ref().map(|m| m.title.clone()).unwrap_or_default();
        let folder = out_dir.join(safe_name(&title, 60));
        std::fs::create_dir_all(&folder)?;
        let mut name = format!("{:02} - {}", clip.index + 1, safe_name(&clip.pick.title, 70));
        if name.trim().is_empty() {
            name = clip.id.clone();
        }
        let out = folder.join(format!("{name}.mp4"));
        self.set_status(id, clip_id, ClipStatus::Exporting).await?;
        let r = self.render_clip(id, &mut clip, Some(&out), remove_silence, cancel, on).await;
        let (c, _) = self
            .store
            .update(id, |p| {
                p.clips.iter_mut().find(|c| c.id == clip_id).map(|c| {
                    match &r {
                        Ok(()) => {
                            c.status = ClipStatus::Exported;
                            c.export_path = Some(out.to_string_lossy().to_string());
                            c.error = None;
                        }
                        Err(e) => {
                            c.status = ClipStatus::Kept;
                            c.error = Some(format!("{e}"));
                        }
                    }
                    c.clone()
                })
            })
            .await?;
        r?;
        c.ok_or_else(|| anyhow!("clip not found"))
    }
}

pub fn safe_name(s: &str, max: usize) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_!'().,&".contains(c) { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let t: String = t.chars().take(max).collect();
    t.trim_end_matches(['.', ' ']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(safe_name("He said: \"WHAT?\" <no>", 60), "He said WHAT no");
        assert_eq!(safe_name("a/b\\c", 60), "a b c");
    }
}
