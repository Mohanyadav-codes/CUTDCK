//! Face detection on sampled frames (SeetaFace via rustface, pure Rust) and
//! simple tracking across frames.

use crate::deps::Tools;
use crate::media::gray_frames;
use crate::types::RectN;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const SAMPLE_FPS: f64 = 5.0;
const SAMPLE_W: u32 = 640;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceObs {
    pub r: RectN,
    pub score: f32,
    /// Mean pixel change in the mouth area since the previous sample (0-255).
    pub motion: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameFaces {
    pub t: f64,
    pub faces: Vec<FaceObs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceData {
    pub fps: f64,
    pub src_w: u32,
    pub src_h: u32,
    pub frames: Vec<FrameFaces>,
}

fn mouth_motion(prev: &[u8], cur: &[u8], w: u32, h: u32, r: &RectN) -> f32 {
    let x0 = ((r.x + r.w * 0.2) * w as f32) as i64;
    let x1 = ((r.x + r.w * 0.8) * w as f32) as i64;
    let y0 = ((r.y + r.h * 0.6) * h as f32) as i64;
    let y1 = ((r.y + r.h * 1.05) * h as f32) as i64;
    let (x0, x1) = (x0.clamp(0, w as i64 - 1), x1.clamp(0, w as i64 - 1));
    let (y0, y1) = (y0.clamp(0, h as i64 - 1), y1.clamp(0, h as i64 - 1));
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in (y0..=y1).step_by(2) {
        for x in (x0..=x1).step_by(2) {
            let i = (y as u32 * w + x as u32) as usize;
            sum += (prev[i] as i32 - cur[i] as i32).unsigned_abs() as u64;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum as f32 / n as f32 }
}

/// Detect faces in a section video.
pub async fn detect(tools: &Tools, video: &Path, src_w: u32, src_h: u32, cancel: &CancellationToken) -> Result<FaceData> {
    let model = tools.face_model.to_string_lossy().to_string();
    let mut det = rustface::create_detector(&model).map_err(|e| anyhow!("face model failed to load: {e}"))?;
    // Faces in a gaming facecam can be small; 640 px wide frames → min 18 px.
    det.set_min_face_size(20);
    det.set_score_thresh(2.0);
    det.set_pyramid_scale_factor(0.8);
    det.set_slide_window_step(4, 4);
    let det = std::sync::Mutex::new(det);
    let mut frames: Vec<FrameFaces> = Vec::new();
    let mut prev: Option<Vec<u8>> = None;
    gray_frames(tools, video, SAMPLE_FPS, SAMPLE_W, src_w, src_h, cancel, |t, w, h, px| {
        let img = rustface::ImageData::new(px, w, h);
        let found = det.lock().unwrap().detect(&img);
        let mut faces: Vec<FaceObs> = found
            .iter()
            .map(|f| {
                let b = f.bbox();
                let r = RectN {
                    x: b.x() as f32 / w as f32,
                    y: b.y() as f32 / h as f32,
                    w: b.width() as f32 / w as f32,
                    h: b.height() as f32 / h as f32,
                };
                let motion = prev.as_ref().map(|p| mouth_motion(p, px, w, h, &r)).unwrap_or(0.0);
                FaceObs { r, score: f.score() as f32, motion }
            })
            .filter(|f| f.r.w > 0.0 && f.r.h > 0.0)
            .collect();
        faces.sort_by(|a, b| b.r.area().partial_cmp(&a.r.area()).unwrap());
        faces.truncate(4);
        frames.push(FrameFaces { t, faces });
        prev = Some(px.to_vec());
    })
    .await?;
    Ok(FaceData { fps: SAMPLE_FPS, src_w, src_h, frames })
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    /// (frame index, observation)
    pub obs: Vec<(usize, FaceObs)>,
    pub presence: f32,
    pub mean: RectN,
    pub jitter: f32,
}

/// Link detections across frames into tracks.
pub fn tracks(fd: &FaceData) -> Vec<Track> {
    let mut ts: Vec<Track> = Vec::new();
    let max_gap = (fd.fps * 2.0) as usize;
    for (fi, fr) in fd.frames.iter().enumerate() {
        let mut used = vec![false; ts.len()];
        for f in &fr.faces {
            let mut best: Option<(usize, f32)> = None;
            for (ti, t) in ts.iter().enumerate() {
                if used[ti] {
                    continue;
                }
                let (lf, last) = t.obs.last().unwrap();
                if fi - lf > max_gap {
                    continue;
                }
                let d = ((last.r.cx() - f.r.cx()).powi(2) + (last.r.cy() - f.r.cy()).powi(2)).sqrt();
                let iou = last.r.iou(&f.r);
                let sim = iou + (0.12 - d).max(0.0) * 4.0;
                if (iou > 0.15 || d < 0.08) && best.map(|b| sim > b.1).unwrap_or(true) {
                    best = Some((ti, sim));
                }
            }
            match best {
                Some((ti, _)) => {
                    used[ti] = true;
                    ts[ti].obs.push((fi, f.clone()));
                }
                None => {
                    ts.push(Track { id: ts.len(), obs: vec![(fi, f.clone())], presence: 0.0, mean: RectN::default(), jitter: 0.0 });
                    used.push(true);
                }
            }
        }
    }
    let n = fd.frames.len().max(1) as f32;
    for t in ts.iter_mut() {
        let k = t.obs.len() as f32;
        t.presence = k / n;
        let mut m = RectN::default();
        for (_, o) in &t.obs {
            m.x += o.r.x / k;
            m.y += o.r.y / k;
            m.w += o.r.w / k;
            m.h += o.r.h / k;
        }
        t.mean = m;
        let var: f32 = t.obs.iter().map(|(_, o)| (o.r.cx() - m.cx()).powi(2) + (o.r.cy() - m.cy()).powi(2)).sum::<f32>() / k;
        t.jitter = var.sqrt();
    }
    // Drop flickers (false positives).
    ts.retain(|t| t.presence >= 0.08 && t.obs.len() >= 3);
    ts.sort_by(|a, b| b.presence.partial_cmp(&a.presence).unwrap());
    for (i, t) in ts.iter_mut().enumerate() {
        t.id = i;
    }
    ts
}
