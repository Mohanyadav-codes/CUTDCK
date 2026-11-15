//! Turning a planned clip into a vertical MP4 with FFmpeg.

use crate::deps::Tools;
use crate::edit::EditList;
use crate::frame::{CAM_SHARE, Framing};
use crate::media::{encoder_args, ffmpeg_progress};
use crate::types::Layout;
use anyhow::Result;
use std::fmt::Write;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub struct RenderSpec<'a> {
    pub tools: &'a Tools,
    pub section: &'a Path,
    pub src_w: u32,
    pub src_h: u32,
    pub fps: f64,
    pub framing: &'a Framing,
    pub edit: &'a EditList,
    /// Captions file name relative to `cwd` (no drive letters in filter args).
    pub ass_name: &'a str,
    /// Fonts dir relative to `cwd`.
    pub fonts_rel: &'a str,
    pub cwd: &'a Path,
    pub out: &'a Path,
    pub preview: bool,
    pub encoder: &'a str,
}

fn even(v: f64) -> i64 {
    ((v / 2.0).round() as i64) * 2
}

/// sendcmd script that moves the crop window frame by frame.
pub fn crop_script(fr: &Framing, src_w: u32, fps: f64) -> String {
    let cw = fr.crop_w as f64 * src_w as f64;
    let mut s = String::new();
    let mut last = i64::MIN;
    for (i, cx) in fr.path.iter().enumerate() {
        let x = ((*cx as f64) * src_w as f64 - cw / 2.0).round().clamp(0.0, src_w as f64 - cw) as i64;
        if x != last {
            let _ = writeln!(s, "{:.4} crop@cam x {};", i as f64 / fps, x);
            last = x;
        }
    }
    s
}

pub fn filter_graph(spec: &RenderSpec, crop_cmd: Option<&str>) -> String {
    let (ow, oh) = if spec.preview { (540i64, 960i64) } else { (1080i64, 1920i64) };
    let w = spec.src_w as f64;
    let h = spec.src_h as f64;
    let fps = spec.fps;
    let fr = spec.framing;
    let mut g = String::new();
    let _ = write!(g, "[0:v]fps={fps},setsar=1");
    match fr.layout {
        Layout::Face | Layout::Center => {
            let cw = even(fr.crop_w as f64 * w).min(w as i64);
            let x0 = fr.path.first().map(|c| ((*c as f64) * w - cw as f64 / 2.0).max(0.0) as i64).unwrap_or(0);
            if let Some(cmd) = crop_cmd {
                let _ = write!(g, ",sendcmd=f={cmd}");
            }
            let _ = write!(g, ",crop@cam=w={cw}:h={}:x={x0}:y=0,scale={ow}:{oh}:flags=lanczos[base];", even(h));
        }
        Layout::Split => {
            let cam = fr.cam.unwrap_or_default();
            let game = fr.game.unwrap_or_default();
            let top = even(oh as f64 * CAM_SHARE);
            let bot = oh - top;
            let r = |v: f32, s: f64| even(v as f64 * s);
            let _ = write!(
                g,
                ",split[c][g];[c]crop={}:{}:{}:{},scale={ow}:{top}:flags=lanczos[top];[g]crop={}:{}:{}:{},scale={ow}:{bot}:flags=lanczos[bot];[top][bot]vstack[base];",
                r(cam.w, w),
                r(cam.h, h),
                r(cam.x, w),
                r(cam.y, h),
                r(game.w, w),
                r(game.h, h),
                r(game.x, w),
                r(game.y, h),
            );
        }
        Layout::Fit | Layout::Auto => {
            let _ = write!(
                g,
                ",split[a][b];[a]scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},gblur=sigma=12,eq=brightness=-0.07:saturation=1.25,scale={ow}:{oh}[bg];[b]scale={ow}:-2:flags=lanczos[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[base];",
                ow / 4,
                oh / 4,
                ow / 4,
                oh / 4,
            );
        }
    }
    // Video: keep the edit list's spans (streamed, frame-exact because spans sit on the frame grid).
    let vsel: Vec<String> = spec.edit.segs.iter().map(|(s, e)| format!("between(t\\,{s:.4}\\,{:.4})", e - 0.5 / fps)).collect();
    let _ = write!(g, "[base]select='{}',setpts=N/FRAME_RATE/TB,subtitles=f={}:fontsdir={}[v];", vsel.join("+"), spec.ass_name, spec.fonts_rel);
    // Audio: sample-exact trims with tiny fades so cuts don't click.
    let k = spec.edit.segs.len();
    let _ = write!(g, "[0:a]aresample=48000,asplit={k}");
    for i in 0..k {
        let _ = write!(g, "[as{i}]");
    }
    g.push(';');
    for (i, (s, e)) in spec.edit.segs.iter().enumerate() {
        let d = e - s;
        let _ = write!(g, "[as{i}]atrim=start={s:.4}:end={e:.4},asetpts=PTS-STARTPTS,afade=t=in:d=0.008,afade=t=out:st={:.4}:d=0.008[ac{i}];", (d - 0.008).max(0.0));
    }
    for i in 0..k {
        let _ = write!(g, "[ac{i}]");
    }
    let _ = write!(g, "concat=n={k}:v=0:a=1");
    if !spec.preview {
        let _ = write!(g, ",loudnorm=I=-14:TP=-1.5:LRA=11,aresample=48000");
    }
    g.push_str("[a]");
    g
}

pub async fn render(spec: &RenderSpec<'_>, cancel: &CancellationToken, on: impl FnMut(f64)) -> Result<()> {
    let crop_cmd = match spec.framing.layout {
        Layout::Face | Layout::Center if spec.framing.path.len() > 1 => {
            std::fs::write(spec.cwd.join("crop.cmd"), crop_script(spec.framing, spec.src_w, spec.fps))?;
            Some("crop.cmd")
        }
        _ => None,
    };
    let graph = filter_graph(spec, crop_cmd);
    let graph_name = if spec.preview { "preview.graph" } else { "final.graph" };
    std::fs::write(spec.cwd.join(graph_name), &graph)?;
    let tmp = spec.out.with_extension("tmp.mp4");
    let mut args: Vec<String> = vec!["-i".into(), spec.section.to_string_lossy().to_string()];
    args.push("-/filter_complex".into());
    args.push(graph_name.into());
    args.extend(["-map", "[v]", "-map", "[a]"].map(String::from));
    if spec.preview {
        args.extend(["-c:v", "libx264", "-preset", "ultrafast", "-crf", "27", "-tune", "fastdecode"].map(String::from));
        args.extend(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k"].map(String::from));
    } else {
        args.extend(encoder_args(spec.encoder));
        args.extend(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k"].map(String::from));
    }
    args.extend(["-r", &format!("{}", spec.fps), "-movflags", "+faststart"].map(String::from));
    args.push(tmp.to_string_lossy().to_string());
    ffmpeg_progress(spec.tools, args, Some(spec.cwd), spec.edit.duration(), cancel, on).await?;
    std::fs::rename(&tmp, spec.out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_shapes() {
        let fr = Framing { layout: Layout::Face, path: vec![0.5, 0.52, 0.55], crop_w: 0.316, cam: None, game: None };
        let edit = EditList { segs: vec![(0.0, 2.0), (3.0, 5.0)] };
        let tools = Tools {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            ytdlp: None,
            deno_dir: None,
            whisper: None,
            fonts_dir: "f".into(),
            face_model: "m".into(),
            models_dir: "m".into(),
        };
        let spec = RenderSpec {
            tools: &tools,
            section: Path::new("s.mp4"),
            src_w: 1920,
            src_h: 1080,
            fps: 30.0,
            framing: &fr,
            edit: &edit,
            ass_name: "captions.ass",
            fonts_rel: "../../fonts",
            cwd: Path::new("."),
            out: Path::new("o.mp4"),
            preview: true,
            encoder: "libx264",
        };
        let g = filter_graph(&spec, Some("crop.cmd"));
        assert!(g.contains("crop@cam=w=606:h=1080"));
        assert!(g.contains("asplit=2"));
        assert!(g.contains("concat=n=2:v=0:a=1[a]"));
        assert!(g.contains("scale=540:960"));
        let s = crop_script(&fr, 1920, 30.0);
        assert_eq!(s.lines().count(), 3);
        assert!(s.starts_with("0.0000 crop@cam x 657;"));
    }
}