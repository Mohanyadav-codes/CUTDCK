//! Animated word-by-word captions as an ASS subtitle file, burned in by libass.

use crate::types::{Layout, Word};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CaptionStyle {
    pub id: &'static str,
    pub name: &'static str,
    pub font: &'static str,
    pub size: u32,
    pub color: &'static str,
    pub highlight: &'static str,
    pub outline: &'static str,
    pub outline_w: f32,
    pub shadow: f32,
    pub upper: bool,
    pub max_words: usize,
    pub max_chars: usize,
    /// "pop" (scale bounce), "fade", "glow"
    pub anim: &'static str,
}

pub const STYLES: &[CaptionStyle] = &[
    CaptionStyle { id: "beast", name: "Beast", font: "Archivo Black", size: 84, color: "#FFFFFF", highlight: "#FFE135", outline: "#000000", outline_w: 7.0, shadow: 0.0, upper: true, max_words: 3, max_chars: 16, anim: "pop" },
    CaptionStyle { id: "hype", name: "Hype", font: "Bangers", size: 116, color: "#FFFFFF", highlight: "#5CFF8A", outline: "#000000", outline_w: 8.0, shadow: 3.0, upper: true, max_words: 2, max_chars: 14, anim: "pop" },
    CaptionStyle { id: "bold", name: "Bold", font: "Anton", size: 108, color: "#FFFFFF", highlight: "#FF5A4E", outline: "#000000", outline_w: 6.0, shadow: 0.0, upper: true, max_words: 2, max_chars: 14, anim: "pop" },
    CaptionStyle { id: "neon", name: "Neon", font: "Bungee", size: 78, color: "#FFFFFF", highlight: "#3CF0FF", outline: "#0A1A3A", outline_w: 5.0, shadow: 0.0, upper: true, max_words: 3, max_chars: 15, anim: "glow" },
    CaptionStyle { id: "clean", name: "Clean", font: "Poppins", size: 66, color: "#FFFFFF", highlight: "#FF5A4E", outline: "#000000", outline_w: 0.0, shadow: 4.0, upper: false, max_words: 4, max_chars: 22, anim: "fade" },
];

pub fn style(id: &str) -> &'static CaptionStyle {
    STYLES.iter().find(|s| s.id == id).unwrap_or(&STYLES[0])
}

/// "#RRGGBB" → ASS "&H00BBGGRR".
fn ass_color(hex: &str, alpha: u8) -> String {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255);
    format!("&H{:02X}{:02X}{:02X}{:02X}", alpha, p(4), p(2), p(0))
}

fn ass_time(t: f64) -> String {
    let cs = (t.max(0.0) * 100.0).round() as u64;
    format!("{}:{:02}:{:02}.{:02}", cs / 360_000, (cs / 6000) % 60, (cs / 100) % 60, cs % 100)
}

fn clean_text(s: &str) -> String {
    s.replace(['{', '}'], "").replace('\\', "/").replace('\n', " ")
}

/// Group words into short caption chunks.
pub fn chunk(words: &[Word], max_words: usize, max_chars: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut chars = 0;
    for (i, w) in words.iter().enumerate() {
        let len = w.w.chars().count();
        let gap = if let Some(&last) = cur.last() { w.s - words[last].e } else { 0.0 };
        if !cur.is_empty() && (cur.len() >= max_words || chars + len + 1 > max_chars || gap > 0.4) {
            out.push(std::mem::take(&mut cur));
            chars = 0;
        }
        cur.push(i);
        chars += len + 1;
        if w.w.ends_with(['.', '?', '!', ',']) {
            out.push(std::mem::take(&mut cur));
            chars = 0;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub struct CaptionInput<'a> {
    pub words: &'a [Word],
    pub style: &'a CaptionStyle,
    pub layout: Layout,
    pub hook: Option<&'a str>,
    pub duration: f64,
}

/// Vertical anchor of captions for a layout (1080x1920 canvas).
pub fn caption_y(layout: Layout) -> u32 {
    match layout {
        Layout::Split => (1920.0 * crate::frame::CAM_SHARE) as u32,
        Layout::Fit => 1500,
        _ => 1340,
    }
}

pub fn build_ass(c: &CaptionInput) -> String {
    let st = c.style;
    let y = caption_y(c.layout);
    let border_style = 1;
    let shadow_col = if st.shadow > 0.0 { ass_color("#000000", 0x60) } else { ass_color("#000000", 0xFF) };
    let mut s = String::new();
    s.push_str("[Script Info]\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: TV.709\n\n");
    s.push_str("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    s.push_str(&format!(
        "Style: Cap,{font},{size},{pc},{sc},{oc},{bc},{bold},0,0,0,100,100,1,0,{bs},{ow},{sh},5,70,70,0,1\n",
        font = st.font,
        size = st.size,
        pc = ass_color(st.color, 0),
        sc = ass_color(st.highlight, 0),
        oc = ass_color(st.outline, 0),
        bc = shadow_col,
        bold = if st.font == "Poppins" { -1 } else { 0 },
        bs = border_style,
        ow = st.outline_w,
        sh = st.shadow,
    ));
    s.push_str(&format!(
        "Style: Hook,Archivo Black,66,{pc},{pc},{box},{box},0,0,0,0,100,100,0,0,3,22,0,5,80,80,0,1\n\n",
        pc = ass_color("#111111", 0),
        box = ass_color("#FFFFFF", 0),
    ));
    s.push_str("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");

    if let Some(h) = c.hook.filter(|h| !h.trim().is_empty()) {
        let hy = if c.layout == Layout::Split { 150 } else { 300 };
        let txt = clean_text(h.trim());
        let end = 3.2f64.min(c.duration);
        s.push_str(&format!(
            "Dialogue: 2,{},{},Hook,,0,0,0,,{{\\an5\\pos(540,{hy})\\fscx70\\fscy70\\t(0,140,\\fscx104\\fscy104)\\t(140,220,\\fscx100\\fscy100)\\fad(0,180)}}{txt}\n",
            ass_time(0.0),
            ass_time(end),
        ));
    }

    let hl = ass_color(st.highlight, 0);
    let base = ass_color(st.color, 0);
    let groups = chunk(c.words, st.max_words, st.max_chars);
    for (gi, g) in groups.iter().enumerate() {
        let first = c.words[g[0]].s;
        let last_end = c.words[*g.last().unwrap()].e;
        let next_start = groups.get(gi + 1).map(|n| c.words[n[0]].s);
        let chunk_end = match next_start {
            Some(ns) if ns - last_end < 0.6 => ns,
            _ => (last_end + 0.3).min(c.duration),
        };
        for (k, &wi) in g.iter().enumerate() {
            let ev_s = if k == 0 { first } else { c.words[wi].s };
            let ev_e = if k + 1 < g.len() { c.words[g[k + 1]].s } else { chunk_end };
            if ev_e - ev_s < 0.02 {
                continue;
            }
            let mut line = format!("{{\\an5\\pos(540,{y})");
            if k == 0 {
                match st.anim {
                    "pop" => line.push_str("\\fscx78\\fscy78\\t(0,110,\\fscx104\\fscy104)\\t(110,170,\\fscx100\\fscy100)"),
                    "glow" => line.push_str("\\fad(70,0)"),
                    _ => line.push_str("\\fad(90,0)"),
                }
            }
            if st.anim == "glow" {
                line.push_str("\\blur1.2");
            }
            line.push('}');
            for (j, &wj) in g.iter().enumerate() {
                if j > 0 {
                    line.push(' ');
                }
                let mut word = clean_text(&c.words[wj].w);
                if st.upper {
                    word = word.to_uppercase();
                }
                if j == k {
                    let fx = match st.anim {
                        "pop" => "\\fscx112\\fscy112\\t(0,90,\\fscx100\\fscy100)",
                        "glow" => "\\3c&HFFF03C&\\blur4\\bord7",
                        _ => "",
                    };
                    line.push_str(&format!("{{\\1c{hl}{fx}}}{word}{{\\1c{base}\\fscx100\\fscy100\\blur{}\\bord{}\\3c{}}}", if st.anim == "glow" { 1.2 } else { 0.0 }, st.outline_w, ass_color(st.outline, 0)));
                } else {
                    line.push_str(&word);
                }
            }
            s.push_str(&format!("Dialogue: 1,{},{},Cap,,0,0,0,,{}\n", ass_time(ev_s), ass_time(ev_e), line));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_times() {
        assert_eq!(ass_color("#FFE135", 0), "&H0035E1FF");
        assert_eq!(ass_time(62.345), "0:01:02.35");
    }

    #[test]
    fn chunks_respect_limits() {
        let ws: Vec<Word> = ["This", "is", "insane.", "Look", "at", "that", "jump"].iter().enumerate().map(|(i, w)| Word { w: w.to_string(), s: i as f64 * 0.3, e: i as f64 * 0.3 + 0.25 }).collect();
        let g = chunk(&ws, 3, 16);
        assert_eq!(g[0], vec![0, 1, 2]);
        assert!(g.iter().all(|c| c.len() <= 3));
        let ass = build_ass(&CaptionInput { words: &ws, style: style("beast"), layout: Layout::Face, hook: Some("He did WHAT?"), duration: 3.0 });
        assert!(ass.contains("Style: Cap,Archivo Black"));
        assert!(ass.contains("INSANE."));
        assert_eq!(ass.matches("Dialogue: 1").count(), 7);
        assert!(ass.contains("Hook"));
    }
}
