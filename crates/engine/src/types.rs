//! Data types shared across the pipeline and persisted in the project folder.

use serde::{Deserialize, Serialize};

/// One spoken word. Times are in seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub w: String,
    pub s: f64,
    pub e: f64,
}

/// A sentence (or a synthetic "loud, no speech" span) used for picking moments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sentence {
    pub id: usize,
    pub s: f64,
    pub e: f64,
    pub text: String,
    /// Index range into the transcript words (`w0..w1`). Empty for synthetic spans.
    pub w0: usize,
    pub w1: usize,
    /// Loudness relative to the whole video, in standard deviations.
    pub loud: f32,
    /// Chat messages per second relative to average (0 when unknown).
    pub chat: f32,
    pub synthetic: bool,
}

/// A moment chosen by the picker (absolute source time).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pick {
    pub start: f64,
    pub end: f64,
    pub title: String,
    pub hook: String,
    pub score: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Auto,
    /// Follow the active face.
    Face,
    /// Facecam on top, gameplay below.
    Split,
    /// Whole frame over a blurred copy.
    Fit,
    /// Static center crop.
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClipStatus {
    #[default]
    Pending,
    Ready,
    Kept,
    Exporting,
    Exported,
    Discarded,
    Failed,
}

/// Normalized rectangle (0..1 of the source frame).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct RectN {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectN {
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn area(&self) -> f32 {
        self.w * self.h
    }
    pub fn iou(&self, o: &RectN) -> f32 {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
        let union = self.area() + o.area() - inter;
        if union <= 0.0 { 0.0 } else { inter / union }
    }
}

/// User edits on top of the automatic clip.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipEdits {
    /// Inclusive word range (indices into `Clip::words`). `None` = automatic.
    pub trim: Option<(usize, usize)>,
    /// Word indices removed by the user.
    pub deleted: Vec<usize>,
    /// Manual crop center override (normalized x) for Face/Center layouts.
    pub crop_x: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub index: usize,
    pub pick: Pick,
    /// Absolute source time where `section.mp4` starts.
    pub section_start: f64,
    pub section_end: f64,
    /// Section video size and frame rate.
    pub src_w: u32,
    pub src_h: u32,
    pub fps: f64,
    /// Words inside the section, in section-local time.
    pub words: Vec<Word>,
    /// Default inclusive word range of the pick.
    pub auto_range: (usize, usize),
    pub layout: Layout,
    /// Layout that `Auto` resolved to.
    pub resolved_layout: Layout,
    pub caption_style: String,
    pub hook_banner: bool,
    pub edits: ClipEdits,
    pub status: ClipStatus,
    /// Output duration after cuts, seconds.
    pub duration: f64,
    pub preview: Option<String>,
    pub thumb: Option<String>,
    pub export_path: Option<String>,
    pub error: Option<String>,
    /// Bumped on each preview render so the UI can bust caches.
    pub rev: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
    /// Original input: file path or URL.
    pub input: String,
    pub is_url: bool,
    pub title: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub was_live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub created: u64,
    pub meta: Option<SourceMeta>,
    pub clips: Vec<Clip>,
    pub stage: String,
    pub done: bool,
    pub error: Option<String>,
}

/// Pipeline stages in order. The UI shows the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Fetching,
    Audio,
    Listening,
    Picking,
    Cutting,
    Done,
}

impl Stage {
    pub fn label(&self) -> &'static str {
        match self {
            Stage::Fetching => "Fetching the video",
            Stage::Audio => "Reading the audio",
            Stage::Listening => "Listening",
            Stage::Picking => "Finding the moments",
            Stage::Cutting => "Cutting clips",
            Stage::Done => "Done",
        }
    }
    pub fn key(&self) -> &'static str {
        match self {
            Stage::Fetching => "fetching",
            Stage::Audio => "audio",
            Stage::Listening => "listening",
            Stage::Picking => "picking",
            Stage::Cutting => "cutting",
            Stage::Done => "done",
        }
    }
}

/// Settings that shape a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSettings {
    pub max_clips: usize,
    pub min_len: f64,
    pub max_len: f64,
    pub caption_style: String,
    pub hook_banner: bool,
    pub layout: Layout,
    pub remove_silence: bool,
    /// Content hint for the picker: "auto", "gaming", "podcast", "vlog".
    pub content: String,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            max_clips: 8,
            min_len: 15.0,
            max_len: 60.0,
            caption_style: "beast".into(),
            hook_banner: true,
            layout: Layout::Auto,
            remove_silence: true,
            content: "auto".into(),
        }
    }
}
