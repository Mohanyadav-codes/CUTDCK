//! Framing: choose a layout and plan the 9:16 crop over time.

use crate::faces::{FaceData, Track, tracks};
use crate::types::{Layout, RectN};

/// Height share of the facecam panel in Split layout.
pub const CAM_SHARE: f64 = 0.40;

#[derive(Debug, Clone)]
pub struct Framing {
    pub layout: Layout,
    /// Crop center x (normalized) per render frame, section-local, for Face/Center.
    pub path: Vec<f32>,
    /// Normalized crop width for Face/Center.
    pub crop_w: f32,
    pub cam: Option<RectN>,
    pub game: Option<RectN>,
}

fn is_vertical(w: u32, h: u32) -> bool {
    (w as f32) / (h.max(1) as f32) < 0.75
}

fn crop_width(w: u32, h: u32) -> f32 {
    ((h as f32 * 9.0 / 16.0) / w.max(1) as f32).min(1.0)
}

/// A static, small, off-center face is a streamer's webcam overlay.
pub fn facecam(ts: &[Track]) -> Option<&Track> {
    ts.iter().find(|t| {
        let off_center = (t.mean.cx() - 0.5).abs() > 0.14 || (t.mean.cy() - 0.5).abs() > 0.18;
        t.presence > 0.4 && t.mean.w < 0.11 && t.jitter < 0.02 && off_center
    })
}

pub fn resolve(requested: Layout, fd: Option<&FaceData>, w: u32, h: u32) -> Layout {
    if requested != Layout::Auto {
        return requested;
    }
    if is_vertical(w, h) {
        return Layout::Center;
    }
    let Some(fd) = fd else { return Layout::Fit };
    let ts = tracks(fd);
    if facecam(&ts).is_some() {
        return Layout::Split;
    }
    // Camera cuts split one person into several short tracks, so count frames
    // with any reasonably sized face instead of relying on one long track.
    let n = fd.frames.len().max(1) as f32;
    let with_face = fd.frames.iter().filter(|f| f.faces.iter().any(|o| o.r.w >= 0.04)).count() as f32;
    if with_face / n >= 0.35 || ts.iter().any(|t| t.presence >= 0.3 && t.mean.w >= 0.06) {
        return Layout::Face;
    }
    Layout::Fit
}

fn clamp_rect(mut r: RectN) -> RectN {
    r.w = r.w.min(1.0);
    r.h = r.h.min(1.0);
    r.x = r.x.clamp(0.0, 1.0 - r.w);
    r.y = r.y.clamp(0.0, 1.0 - r.h);
    r
}

/// Facecam and gameplay rectangles for the Split layout.
pub fn split_rects(fd: Option<&FaceData>, w: u32, h: u32) -> (RectN, RectN) {
    let aspect_px = |share: f64| 1080.0 / (1920.0 * share);
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;
    let cam_aspect = aspect_px(CAM_SHARE) as f32; // width / height of the top panel
    let ts = fd.map(tracks).unwrap_or_default();
    let cam = match facecam(&ts).or_else(|| ts.first()) {
        Some(t) => {
            let fw_px = t.mean.w * wf;
            let cw_px = (fw_px * 3.2).max(wf * 0.18);
            let ch_px = cw_px / cam_aspect;
            let cx = t.mean.cx() * wf;
            let cy = (t.mean.cy() + t.mean.h * 0.25) * hf;
            clamp_rect(RectN { x: (cx - cw_px / 2.0) / wf, y: (cy - ch_px / 2.0) / hf, w: cw_px / wf, h: ch_px / hf })
        }
        None => {
            let cw = 0.32;
            clamp_rect(RectN { x: 0.0, y: 0.0, w: cw, h: cw * wf / cam_aspect / hf })
        }
    };
    let game_aspect = aspect_px(1.0 - CAM_SHARE) as f32;
    let gw_px = (hf * game_aspect).min(wf);
    let game = clamp_rect(RectN { x: (wf - gw_px) / 2.0 / wf, y: 0.0, w: gw_px / wf, h: 1.0 });
    (cam, game)
}

/// Pick the face to follow at each sample, with hysteresis between speakers.
fn targets(fd: &FaceData, ts: &[Track]) -> Vec<(f32, Option<usize>)> {
    let n = fd.frames.len();
    let main: Vec<&Track> = ts.iter().filter(|t| t.presence >= 0.08).collect();
    let mut by_frame: Vec<Vec<(usize, &crate::faces::FaceObs)>> = vec![Vec::new(); n];
    for t in &main {
        for (fi, o) in &t.obs {
            by_frame[*fi].push((t.id, o));
        }
    }
    // Speaking activity: mouth motion smoothed over ~1 s.
    let win = (fd.fps as usize).max(1);
    let activity = |tid: usize, fi: usize| -> f32 {
        let a = fi.saturating_sub(win / 2);
        let b = (fi + win / 2 + 1).min(n);
        let mut s = 0.0;
        let mut k = 0.0;
        for f in by_frame.iter().take(b).skip(a) {
            if let Some((_, o)) = f.iter().find(|(id, _)| *id == tid) {
                s += o.motion;
                k += 1.0;
            }
        }
        if k == 0.0 { 0.0 } else { s / k }
    };
    let hold = (fd.fps * 1.5) as usize;
    let mut cur: Option<usize> = None;
    let mut lead: Option<(usize, usize)> = None;
    let mut last_x = 0.5f32;
    let mut out = Vec::with_capacity(n);
    for fi in 0..n {
        let present = &by_frame[fi];
        if present.is_empty() {
            out.push((last_x, cur));
            continue;
        }
        let best = present
            .iter()
            .map(|(id, o)| (*id, activity(*id, fi) + o.r.w * 20.0))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|x| x.0)
            .unwrap();
        match cur {
            None => cur = Some(best),
            Some(c) if c != best => {
                let cur_present = present.iter().any(|(id, _)| *id == c);
                match lead {
                    Some((id, since)) if id == best => {
                        if fi - since >= hold || !cur_present {
                            cur = Some(best);
                            lead = None;
                        }
                    }
                    _ => {
                        if !cur_present {
                            cur = Some(best);
                        } else {
                            lead = Some((best, fi));
                        }
                    }
                }
            }
            _ => lead = None,
        }
        if let Some((_, o)) = present.iter().find(|(id, _)| Some(*id) == cur) {
            last_x = o.r.cx();
        }
        out.push((last_x, cur));
    }
    out
}

/// Plan framing for `duration` seconds of section video at `fps`.
pub fn plan(layout: Layout, fd: Option<&FaceData>, w: u32, h: u32, fps: f64, duration: f64, manual_x: Option<f32>) -> Framing {
    let frames = (duration * fps).ceil().max(1.0) as usize;
    let cw = crop_width(w, h);
    let half = cw / 2.0;
    let clampx = |x: f32| x.clamp(half, 1.0 - half);
    match layout {
        Layout::Split => {
            let (cam, game) = split_rects(fd, w, h);
            Framing { layout, path: vec![], crop_w: cw, cam: Some(cam), game: Some(game) }
        }
        Layout::Fit | Layout::Auto => Framing { layout: Layout::Fit, path: vec![], crop_w: cw, cam: None, game: None },
        Layout::Center => {
            let x = clampx(manual_x.unwrap_or(0.5));
            Framing { layout, path: vec![x; frames], crop_w: cw, cam: None, game: None }
        }
        Layout::Face => {
            if let Some(x) = manual_x {
                return Framing { layout, path: vec![clampx(x); frames], crop_w: cw, cam: None, game: None };
            }
            let Some(fd) = fd else {
                return Framing { layout: Layout::Center, path: vec![0.5; frames], crop_w: cw, cam: None, game: None };
            };
            let ts = tracks(fd);
            let tg = targets(fd, &ts);
            let mut path = Vec::with_capacity(frames);
            let dead = 0.06f32; // 6% of the frame width
            let omega = 3.5f32; // spring stiffness (rad/s)
            let dt = 1.0 / fps as f32;
            let mut p = clampx(tg.first().map(|t| t.0).unwrap_or(0.5));
            let mut v = 0.0f32;
            let mut prev_track = tg.first().and_then(|t| t.1);
            for i in 0..frames {
                let t = i as f64 / fps;
                let si = ((t * fd.fps) as usize).min(tg.len().saturating_sub(1));
                let (tx, track) = if tg.is_empty() { (0.5, None) } else { tg[si] };
                // Interpolate between samples for smooth targets.
                let tx = if si + 1 < tg.len() && tg[si + 1].1 == track {
                    let f = (t * fd.fps - si as f64) as f32;
                    tx + (tg[si + 1].0 - tx) * f
                } else {
                    tx
                };
                let tx = clampx(tx);
                if track != prev_track && track.is_some() && prev_track.is_some() {
                    // Speaker change: hard cut, no pan.
                    p = tx;
                    v = 0.0;
                } else if (tx - p).abs() > dead || v.abs() > 0.001 {
                    // Critically damped spring toward the target.
                    let target = if (tx - p).abs() > dead { tx } else { p };
                    let a = omega * omega * (target - p) - 2.0 * omega * v;
                    v += a * dt;
                    p += v * dt;
                    if (target - p).abs() < 0.002 && v.abs() < 0.01 {
                        v = 0.0;
                    }
                }
                prev_track = track.or(prev_track);
                p = clampx(p);
                path.push(p);
            }
            Framing { layout, path, crop_w: cw, cam: None, game: None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faces::{FaceObs, FrameFaces};

    fn fd_with(face: RectN, n: usize) -> FaceData {
        FaceData {
            fps: 5.0,
            src_w: 1920,
            src_h: 1080,
            frames: (0..n).map(|i| FrameFaces { t: i as f64 / 5.0, faces: vec![FaceObs { r: face, score: 5.0, motion: 2.0 }] }).collect(),
        }
    }

    #[test]
    fn webcam_overlay_means_split() {
        let fd = fd_with(RectN { x: 0.05, y: 0.70, w: 0.06, h: 0.11 }, 50);
        assert_eq!(resolve(Layout::Auto, Some(&fd), 1920, 1080), Layout::Split);
        let (cam, game) = split_rects(Some(&fd), 1920, 1080);
        assert!(cam.x >= 0.0 && cam.x + cam.w <= 1.0 && cam.y + cam.h <= 1.0001);
        assert!((game.h - 1.0).abs() < 1e-4);
    }

    #[test]
    fn big_face_means_face_and_path_is_valid() {
        let fd = fd_with(RectN { x: 0.6, y: 0.2, w: 0.15, h: 0.27 }, 50);
        assert_eq!(resolve(Layout::Auto, Some(&fd), 1920, 1080), Layout::Face);
        let f = plan(Layout::Face, Some(&fd), 1920, 1080, 30.0, 10.0, None);
        assert_eq!(f.path.len(), 300);
        let half = f.crop_w / 2.0;
        assert!(f.path.iter().all(|x| *x >= half - 1e-4 && *x <= 1.0 - half + 1e-4));
        // Settles near the face.
        assert!((f.path.last().unwrap() - 0.675).abs() < 0.03);
    }

    #[test]
    fn no_faces_means_fit_and_vertical_means_center() {
        let fd = FaceData { fps: 5.0, src_w: 1920, src_h: 1080, frames: vec![FrameFaces { t: 0.0, faces: vec![] }; 20] };
        assert_eq!(resolve(Layout::Auto, Some(&fd), 1920, 1080), Layout::Fit);
        assert_eq!(resolve(Layout::Auto, None, 1080, 1920), Layout::Center);
    }
}
