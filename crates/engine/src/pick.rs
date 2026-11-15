//! Finding the moments worth posting.

use crate::ai::LlmClient;
use crate::types::{JobSettings, Pick, Sentence, Word};
use anyhow::Result;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub const HOP: f64 = 0.25; // loudness curve resolution, seconds

/// Loudness statistics used to score moments.
pub struct Energy {
    pub curve: Vec<f32>,
    pub mean: f32,
    pub std: f32,
}

impl Energy {
    pub fn new(curve: Vec<f32>) -> Self {
        let voiced: Vec<f32> = curve.iter().copied().filter(|v| *v > -55.0).collect();
        let n = voiced.len().max(1) as f32;
        let mean = voiced.iter().sum::<f32>() / n;
        let var = voiced.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
        Self { curve, mean, std: var.sqrt().max(1.0) }
    }
    fn idx(&self, t: f64) -> usize {
        ((t / HOP) as usize).min(self.curve.len().saturating_sub(1))
    }
    /// Mean z-score over a span.
    pub fn z(&self, s: f64, e: f64) -> f32 {
        if self.curve.is_empty() {
            return 0.0;
        }
        let (a, b) = (self.idx(s), self.idx(e).max(self.idx(s) + 1).min(self.curve.len()));
        let slice = &self.curve[a..b.max(a + 1).min(self.curve.len())];
        let m = slice.iter().sum::<f32>() / slice.len().max(1) as f32;
        (m - self.mean) / self.std
    }
    /// Peak (90th percentile) z-score over a span, for hype moments.
    pub fn peak_z(&self, s: f64, e: f64) -> f32 {
        if self.curve.is_empty() {
            return 0.0;
        }
        let (a, b) = (self.idx(s), self.idx(e).min(self.curve.len()));
        if b <= a {
            return self.z(s, e);
        }
        let mut v: Vec<f32> = self.curve[a..b].to_vec();
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let p = v[((v.len() as f32) * 0.9) as usize % v.len()];
        (p - self.mean) / self.std
    }
    pub fn duration(&self) -> f64 {
        self.curve.len() as f64 * HOP
    }
}

/// Chat activity (messages per second) relative to the average.
pub struct Chat {
    pub times: Vec<f64>,
    pub rate: f64,
}

impl Chat {
    pub fn new(mut times: Vec<f64>, duration: f64) -> Self {
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let rate = times.len() as f64 / duration.max(1.0);
        Self { times, rate }
    }
    pub fn rel(&self, s: f64, e: f64) -> f32 {
        if self.times.is_empty() || e <= s {
            return 0.0;
        }
        let a = self.times.partition_point(|t| *t < s);
        let b = self.times.partition_point(|t| *t < e);
        let r = (b - a) as f64 / (e - s);
        ((r / self.rate.max(1e-6)) - 1.0) as f32
    }
}

fn ends_sentence(w: &str) -> bool {
    w.ends_with('.') || w.ends_with('?') || w.ends_with('!')
}

/// Group words into sentences and add "loud, no talking" spans.
pub fn sentences(words: &[Word], energy: &Energy, chat: &Chat) -> Vec<Sentence> {
    let mut out: Vec<Sentence> = Vec::new();
    let mut start = 0usize;
    for i in 0..words.len() {
        let gap = words.get(i + 1).map(|n| n.s - words[i].e).unwrap_or(10.0);
        let len = i + 1 - start;
        if ends_sentence(&words[i].w) || gap > 0.65 || len >= 28 || i + 1 == words.len() {
            let s = words[start].s;
            let e = words[i].e;
            let text = words[start..=i].iter().map(|w| w.w.as_str()).collect::<Vec<_>>().join(" ");
            out.push(Sentence { id: 0, s, e, text, w0: start, w1: i + 1, loud: energy.z(s, e), chat: chat.rel(s, e), synthetic: false });
            start = i + 1;
        }
    }
    // Loud stretches without speech (gaming action, crowd reactions).
    let mut extra = Vec::new();
    let mut prev_end = 0.0;
    let mut gaps: Vec<(f64, f64)> = Vec::new();
    for s in &out {
        if s.s - prev_end >= 4.0 {
            gaps.push((prev_end, s.s));
        }
        prev_end = s.e;
    }
    if energy.duration() - prev_end >= 4.0 {
        gaps.push((prev_end, energy.duration()));
    }
    for (gs, ge) in gaps {
        let mut t = gs;
        while t + 3.0 <= ge {
            let e = (t + 8.0).min(ge);
            let z = energy.z(t, e);
            if z > 0.8 {
                extra.push(Sentence { id: 0, s: t, e, text: "(no talking, loud moment)".into(), w0: 0, w1: 0, loud: z, chat: chat.rel(t, e), synthetic: true });
            }
            t = e;
        }
    }
    out.extend(extra);
    out.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap());
    for (i, s) in out.iter_mut().enumerate() {
        s.id = i;
    }
    out
}

/// For very long videos, choose the most active stretches to transcribe.
pub fn choose_regions(energy: &Energy, chat: &Chat, duration: f64, budget: f64) -> Vec<(f64, f64)> {
    if duration <= budget * 1.25 {
        return vec![(0.0, duration)];
    }
    let bucket = 60.0;
    let n = (duration / bucket).ceil() as usize;
    let mut scored: Vec<(usize, f32)> = (0..n)
        .map(|i| {
            let s = i as f64 * bucket;
            let e = (s + bucket).min(duration);
            (i, energy.peak_z(s, e) * 0.6 + energy.z(s, e) * 0.4 + chat.rel(s, e).min(4.0) * 0.8)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let merge = |mut regions: Vec<(f64, f64)>| -> Vec<(f64, f64)> {
        regions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for r in regions {
            if let Some(last) = merged.last_mut() {
                if r.0 <= last.1 + 5.0 {
                    last.1 = last.1.max(r.1);
                    continue;
                }
            }
            merged.push(r);
        }
        merged
    };
    let mut regions: Vec<(f64, f64)> = Vec::new();
    for (i, _) in scored {
        let c = i as f64 * bucket + bucket / 2.0;
        if regions.iter().any(|r| c >= r.0 && c <= r.1) {
            continue;
        }
        let r = ((c - 120.0).max(0.0), (c + 120.0).min(duration));
        regions = merge([regions, vec![r]].concat());
        if regions.iter().map(|r| r.1 - r.0).sum::<f64>() >= budget {
            break;
        }
    }
    regions
}

fn fmt_time(t: f64) -> String {
    let t = t.max(0.0) as u64;
    if t >= 3600 { format!("{}:{:02}:{:02}", t / 3600, (t / 60) % 60, t % 60) } else { format!("{:02}:{:02}", t / 60, t % 60) }
}

fn system_prompt(settings: &JobSettings, title: &str) -> String {
    let kind = match settings.content.as_str() {
        "gaming" => "a gaming video or live stream",
        "podcast" => "a podcast or interview",
        "vlog" => "a vlog or entertainment video",
        _ => "a long video (it may be gaming, a stream, a podcast, a challenge video or a vlog)",
    };
    format!(
        "You are the best short-form video editor on YouTube Shorts, TikTok and Reels. \
You watch long videos and find the moments that make people stop scrolling, watch to the end and share. \
This is {kind} titled \"{title}\". You answer only with JSON."
    )
}

fn user_prompt(settings: &JobSettings, lines: &str, want: usize) -> String {
    format!(
        r#"Find the best moments for vertical shorts in this part of the transcript.

Each line is: id [time] text. Markers (never copy them into titles or hooks): LOUD = shouting, hype or big sound. CHAT = chat exploded. "(no talking, loud moment)" = action without speech.

Rules:
- Each clip is {min:.0} to {max:.0} seconds long (sweet spot 25-45 s).
- It must start with a hook in the first 2 seconds: a bold claim, a question, a reaction, a conflict, a reveal or a funny or shocking moment. Never start mid-thought.
- It must make sense with no context.
- It must end right after the payoff, punchline or reaction. Don't trail off.
- For gaming: clutch plays, fails, rage, jump scares, funny reactions, big wins. LOUD and CHAT lines are strong signals.
- Skip intros, sponsor reads, "like and subscribe", outros and housekeeping.
- Clips must not overlap.

Return up to {want} clips, best first:
{{"clips":[{{"start":<first line id>,"end":<last line id>,"title":"<max 8 words, curiosity-driven, true to the clip>","hook":"<max 6 words for an on-screen banner>","score":<0-100 chance it goes viral>,"reason":"<one short sentence>"}}]}}

Transcript:
{lines}"#,
        min = settings.min_len,
        max = settings.max_len,
    )
}

fn render_lines(sents: &[Sentence]) -> String {
    let mut s = String::new();
    for x in sents {
        let mut flags = String::new();
        if x.loud > 1.2 {
            flags.push_str(" LOUD");
        }
        if x.chat > 1.5 {
            flags.push_str(" CHAT");
        }
        s.push_str(&format!("{} [{}] {}{}\n", x.id, fmt_time(x.s), x.text, flags));
    }
    s
}

/// Snap an id range to clip times that respect the length limits.
fn span_for(sents: &[Sentence], mut a: usize, mut b: usize, min: f64, max: f64) -> Option<(f64, f64, usize, usize)> {
    if sents.is_empty() {
        return None;
    }
    a = a.min(sents.len() - 1);
    b = b.clamp(a, sents.len() - 1);
    // Too long: drop trailing sentences.
    while b > a && sents[b].e - sents[a].s > max {
        b -= 1;
    }
    // Too short: extend forward, then backward.
    while sents[b].e - sents[a].s < min && b + 1 < sents.len() && sents[b + 1].e - sents[a].s <= max {
        b += 1;
    }
    while sents[b].e - sents[a].s < min && a > 0 && sents[b].e - sents[a - 1].s <= max {
        a -= 1;
    }
    let s = (sents[a].s - 0.2).max(0.0);
    let mut e = sents[b].e + 0.45;
    if e - s > max + 1.0 {
        e = s + max;
    }
    if e - s < min * 0.6 {
        return None;
    }
    Some((s, e, a, b))
}

pub fn heuristic_score(sents: &[Sentence], a: usize, b: usize, energy: &Energy, chat: &Chat) -> f32 {
    let s = sents[a].s;
    let e = sents[b].e;
    let words: usize = sents[a..=b].iter().map(|x| x.w1 - x.w0).sum();
    let wps = words as f64 / (e - s).max(1.0);
    let text: String = sents[a..=b].iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ");
    let q = text.matches('?').count().min(3) as f32;
    let ex = text.matches('!').count().min(4) as f32;
    let hook = sents[a].loud.max(0.0) + if sents[a].text.contains('?') { 0.6 } else { 0.0 };
    energy.peak_z(s, e).clamp(-1.0, 3.0) * 1.2
        + chat.rel(s, e).clamp(0.0, 4.0) * 0.8
        + (wps as f32 - 2.0).clamp(-1.5, 1.5) * 0.5
        + q * 0.25
        + ex * 0.2
        + hook * 0.5
}

/// Remove our transcript markers and odd punctuation from AI-written labels.
fn clean_label(s: &str) -> String {
    let s = s.replace(['\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}'], "-").replace('\u{2014}', " - ");
    let words: Vec<&str> = s.split_whitespace().filter(|w| !matches!(w.trim_matches(|c: char| !c.is_alphanumeric()), "LOUD" | "CHAT")).collect();
    words.join(" ").trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace()).to_string()
}

fn to_score(h: f32) -> f32 {
    // Map a heuristic value to roughly 0-100.
    (50.0 + h * 12.0).clamp(5.0, 95.0)
}

fn short(text: &str, words: usize) -> String {
    let t: Vec<&str> = text.split_whitespace().take(words).collect();
    let mut s = t.join(" ");
    s = s.trim_end_matches(|c: char| c == ',' || c == '.').to_string();
    s
}

/// Rank without an LLM: score every sentence-aligned window, keep the best non-overlapping.
pub fn heuristic_picks(sents: &[Sentence], energy: &Energy, chat: &Chat, settings: &JobSettings) -> Vec<Pick> {
    let mut cands: Vec<(f32, f64, f64, usize, usize)> = Vec::new();
    let target = ((settings.min_len + settings.max_len) / 2.0).min(40.0);
    for a in 0..sents.len() {
        if sents[a].synthetic && sents[a].loud < 1.0 {
            continue;
        }
        let mut b = a;
        while b + 1 < sents.len() && sents[b].e - sents[a].s < target {
            b += 1;
        }
        if let Some((s, e, a2, b2)) = span_for(sents, a, b, settings.min_len, settings.max_len) {
            cands.push((heuristic_score(sents, a2, b2, energy, chat), s, e, a2, b2));
        }
    }
    cands.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
    let mut picks: Vec<Pick> = Vec::new();
    for (h, s, e, a, _b) in cands {
        if picks.len() >= settings.max_clips {
            break;
        }
        if picks.iter().any(|p| overlap(p.start, p.end, s, e) > 0.15) {
            continue;
        }
        let first = sents[a..].iter().find(|x| !x.synthetic).map(|x| x.text.clone()).unwrap_or_else(|| "Big moment".into());
        picks.push(Pick { start: s, end: e, title: short(&first, 8), hook: String::new(), score: to_score(h), reason: "Loud, dense, high-energy stretch.".into() });
    }
    picks
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    let inter = (a1.min(b1) - a0.max(b0)).max(0.0);
    inter / (a1 - a0).min(b1 - b0).max(0.1)
}

/// Pick moments with the LLM, window by window, then merge and rank.
pub async fn llm_picks(
    llm: &LlmClient,
    sents: &[Sentence],
    energy: &Energy,
    chat: &Chat,
    settings: &JobSettings,
    title: &str,
    cancel: &CancellationToken,
    on: &(dyn Fn(f64) + Send + Sync),
) -> Result<Vec<Pick>> {
    if sents.is_empty() {
        return Ok(vec![]);
    }
    let win = llm.window_min * 60.0;
    let mut windows: Vec<(usize, usize)> = Vec::new();
    let mut a = 0usize;
    while a < sents.len() {
        let t0 = sents[a].s;
        let mut b = a;
        while b + 1 < sents.len() && sents[b + 1].e - t0 <= win {
            b += 1;
        }
        windows.push((a, b));
        if b + 1 >= sents.len() {
            break;
        }
        // One minute of overlap so moments at the seams aren't lost.
        let mut na = b;
        while na > a + 1 && sents[b].e - sents[na].s < 60.0 {
            na -= 1;
        }
        a = na.max(a + 1);
    }
    let per = ((settings.max_clips as f64 * 1.6) / windows.len() as f64).ceil().max(3.0) as usize;
    let system = system_prompt(settings, title);
    let mut cands: Vec<(f32, Pick)> = Vec::new();
    let mut failures = 0;
    for (i, (a, b)) in windows.iter().enumerate() {
        let lines = render_lines(&sents[*a..=*b]);
        let user = user_prompt(settings, &lines, per);
        match llm.json(&system, &user, cancel).await {
            Ok(v) => {
                for c in v["clips"].as_array().cloned().unwrap_or_default() {
                    if let Some(p) = parse_clip(&c, sents, *a, *b, energy, chat, settings) {
                        cands.push(p);
                    }
                }
            }
            Err(e) => {
                if e.to_string() == "cancelled" {
                    return Err(e);
                }
                tracing::warn!("picking window {i} failed: {e}");
                failures += 1;
                if failures >= 2 && cands.is_empty() {
                    return Err(e);
                }
            }
        }
        on((i + 1) as f64 / windows.len() as f64);
    }
    cands.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
    let mut out: Vec<Pick> = Vec::new();
    for (_, p) in cands {
        if out.len() >= settings.max_clips {
            break;
        }
        if out.iter().any(|o| overlap(o.start, o.end, p.start, p.end) > 0.3) {
            continue;
        }
        out.push(p);
    }
    Ok(out)
}

fn parse_clip(c: &Value, sents: &[Sentence], wa: usize, wb: usize, energy: &Energy, chat: &Chat, settings: &JobSettings) -> Option<(f32, Pick)> {
    let num = |k: &str| c.get(k).and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())));
    let a = num("start")? as usize;
    let b = num("end").unwrap_or(a as u64) as usize;
    if a < wa.saturating_sub(2) || a > wb || b < a {
        return None;
    }
    let (s, e, a2, b2) = span_for(sents, a, b, settings.min_len, settings.max_len)?;
    let llm_score = c.get("score").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(60.0) as f32;
    let h = heuristic_score(sents, a2, b2, energy, chat);
    let combined = (llm_score.clamp(0.0, 100.0) * 0.8 + to_score(h) * 0.2).clamp(1.0, 99.0);
    let text = |k: &str| clean_label(c.get(k).and_then(|v| v.as_str()).unwrap_or(""));
    let first = sents[a2..].iter().find(|x| !x.synthetic).map(|x| x.text.clone()).unwrap_or_default();
    let mut title = text("title");
    if title.is_empty() {
        title = short(&first, 8);
    }
    let mut hook = text("hook");
    if hook.is_empty() {
        hook = short(&title, 6);
    }
    Some((combined, Pick { start: s, end: e, title, hook, score: combined.round(), reason: text("reason") }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(n: usize) -> Vec<Word> {
        (0..n)
            .map(|i| {
                let s = i as f64 * 0.4;
                let w = if i % 10 == 9 { "end.".to_string() } else { format!("w{i}") };
                Word { w, s, e: s + 0.3 }
            })
            .collect()
    }

    #[test]
    fn labels_are_cleaned() {
        assert_eq!(clean_label("Jump! LOUD chaos"), "Jump! chaos");
        assert_eq!(clean_label("Tug\u{2011}of\u{2011}War"), "Tug-of-War");
    }

    #[test]
    fn builds_sentences() {
        let ws = words(100);
        let en = Energy::new(vec![-20.0; 200]);
        let ch = Chat::new(vec![], 40.0);
        let s = sentences(&ws, &en, &ch);
        assert_eq!(s.iter().filter(|x| !x.synthetic).count(), 10);
        assert_eq!(s[0].w1, 10);
    }

    #[test]
    fn span_limits() {
        let ws = words(400);
        let en = Energy::new(vec![-20.0; 800]);
        let ch = Chat::new(vec![], 160.0);
        let s = sentences(&ws, &en, &ch);
        let (a, b, _, _) = span_for(&s, 0, 0, 15.0, 60.0).unwrap();
        assert!(b - a >= 15.0 - 1.0 && b - a <= 61.0, "{a} {b}");
        let (a, b, _, _) = span_for(&s, 0, s.len() - 1, 15.0, 60.0).unwrap();
        assert!(b - a <= 61.0);
    }

    #[test]
    fn heuristic_returns_non_overlapping() {
        let ws = words(1500);
        let mut curve = vec![-25.0f32; 2400];
        for v in curve.iter_mut().skip(800).take(80) {
            *v = -8.0;
        }
        let en = Energy::new(curve);
        let ch = Chat::new(vec![], 600.0);
        let s = sentences(&ws, &en, &ch);
        let st = JobSettings::default();
        let p = heuristic_picks(&s, &en, &ch, &st);
        assert!(!p.is_empty());
        for i in 0..p.len() {
            for j in i + 1..p.len() {
                assert!(overlap(p[i].start, p[i].end, p[j].start, p[j].end) <= 0.15);
            }
        }
        // The loud part (t = 200-220 s) should be in the top pick.
        assert!(p[0].start <= 215.0 && p[0].end >= 200.0, "{:?}", p[0]);
    }

    #[test]
    fn long_video_regions() {
        let en = Energy::new(vec![-20.0; (4.0 * 3600.0 / HOP) as usize]);
        let ch = Chat::new(vec![], 4.0 * 3600.0);
        let r = choose_regions(&en, &ch, 4.0 * 3600.0, 3600.0);
        let total: f64 = r.iter().map(|x| x.1 - x.0).sum();
        assert!(total <= 3600.0 + 241.0 && total >= 3000.0, "{total}");
    }
}
