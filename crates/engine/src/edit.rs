//! The edit list: which parts of a section survive (silences, filler words and
//! user deletions are cut). Everything after this stage works in output time.

use crate::types::Word;
use serde::{Deserialize, Serialize};

const FILLERS: &[&str] = &["um", "uh", "umm", "uhh", "erm", "er", "hmm", "mm", "uhm", "ah"];

pub fn is_filler(w: &str) -> bool {
    let t: String = w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    FILLERS.contains(&t.as_str())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EditList {
    /// Kept spans in section-local seconds, sorted, non-overlapping.
    pub segs: Vec<(f64, f64)>,
}

pub struct EditParams<'a> {
    pub words: &'a [Word],
    /// Base span in section-local time.
    pub start: f64,
    pub end: f64,
    pub deleted: &'a [usize],
    pub remove_silence: bool,
    pub fps: f64,
    /// Is the audio quiet over this section-local span?
    pub quiet: &'a dyn Fn(f64, f64) -> bool,
}

fn snap(t: f64, fps: f64) -> f64 {
    (t * fps).round() / fps
}

impl EditList {
    pub fn build(p: &EditParams) -> EditList {
        let mut cuts: Vec<(f64, f64)> = Vec::new();
        let inside: Vec<(usize, &Word)> = p.words.iter().enumerate().filter(|(_, w)| w.e > p.start && w.s < p.end).collect();
        for (i, w) in &inside {
            if p.deleted.contains(i) || (p.remove_silence && is_filler(&w.w)) {
                cuts.push((w.s, w.e));
            }
        }
        if p.remove_silence {
            let kept: Vec<&Word> = inside.iter().filter(|(i, w)| !p.deleted.contains(i) && !is_filler(&w.w)).map(|(_, w)| *w).collect();
            for pair in kept.windows(2) {
                let (a, b) = (pair[0].e, pair[1].s);
                if b - a > 0.7 && (p.quiet)(a, b) {
                    cuts.push((a + 0.15, b - 0.15));
                }
            }
            // Dead air at the edges.
            if let Some(first) = kept.first() {
                if first.s - p.start > 0.8 && (p.quiet)(p.start, first.s) {
                    cuts.push((p.start, first.s - 0.2));
                }
            }
            if let Some(last) = kept.last() {
                if p.end - last.e > 1.0 && (p.quiet)(last.e, p.end) {
                    cuts.push((last.e + 0.45, p.end));
                }
            }
        }
        cuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut segs = Vec::new();
        let mut t = p.start;
        for (cs, ce) in cuts {
            if ce <= t {
                continue;
            }
            if cs > t {
                segs.push((t, cs.min(p.end)));
            }
            t = t.max(ce);
            if t >= p.end {
                break;
            }
        }
        if t < p.end {
            segs.push((t, p.end));
        }
        // Snap to the frame grid so audio and video cut at the same instant.
        let min = 2.0 / p.fps;
        let mut out: Vec<(f64, f64)> = Vec::new();
        for (s, e) in segs {
            let (s, e) = (snap(s.max(0.0), p.fps), snap(e, p.fps));
            if e - s < min {
                continue;
            }
            if let Some(last) = out.last_mut() {
                if s - last.1 < 0.12 {
                    last.1 = e;
                    continue;
                }
            }
            out.push((s, e));
        }
        if out.is_empty() {
            out.push((snap(p.start, p.fps), snap(p.end.max(p.start + 1.0), p.fps)));
        }
        EditList { segs: out }
    }

    pub fn duration(&self) -> f64 {
        self.segs.iter().map(|(s, e)| e - s).sum()
    }

    /// Section-local time → output time, if the moment survives.
    pub fn map(&self, t: f64) -> Option<f64> {
        let mut acc = 0.0;
        for &(s, e) in &self.segs {
            if t >= s && t < e {
                return Some(acc + t - s);
            }
            acc += e - s;
        }
        None
    }

    /// Like `map`, but moments inside a cut land on the next kept moment.
    pub fn map_clamped(&self, t: f64) -> f64 {
        let mut acc = 0.0;
        for &(s, e) in &self.segs {
            if t < s {
                return acc;
            }
            if t < e {
                return acc + t - s;
            }
            acc += e - s;
        }
        acc
    }

    /// Words that survive, retimed to output time.
    pub fn retime(&self, words: &[Word], deleted: &[usize]) -> Vec<Word> {
        let mut out = Vec::new();
        for (i, w) in words.iter().enumerate() {
            if deleted.contains(&i) {
                continue;
            }
            let mid = (w.s + w.e) / 2.0;
            if self.map(mid).is_none() {
                continue;
            }
            let s = self.map_clamped(w.s);
            let e = self.map_clamped(w.e).max(s + 0.05);
            out.push(Word { w: w.w.clone(), s, e });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(t: &str, s: f64, e: f64) -> Word {
        Word { w: t.into(), s, e }
    }

    #[test]
    fn cuts_silence_and_fillers() {
        let words = vec![w("Hello", 0.0, 0.5), w("um", 0.6, 0.9), w("world", 1.0, 1.5), w("again", 4.0, 4.5)];
        let quiet = |_: f64, _: f64| true;
        let e = EditList::build(&EditParams { words: &words, start: 0.0, end: 5.0, deleted: &[], remove_silence: true, fps: 30.0, quiet: &quiet });
        // Filler removed, 2.5 s gap shortened, tail trimmed.
        assert!(e.segs.len() >= 2, "{:?}", e.segs);
        assert!(e.duration() < 3.5, "{}", e.duration());
        assert!(e.map(0.75).is_none());
        assert!(e.map(2.5).is_none());
        let rt = e.retime(&words, &[]);
        assert_eq!(rt.iter().map(|x| x.w.as_str()).collect::<Vec<_>>(), vec!["Hello", "world", "again"]);
        assert!(rt.windows(2).all(|p| p[1].s >= p[0].s));
        // Frame grid.
        for (s, en) in &e.segs {
            assert!(((s * 30.0).round() - s * 30.0).abs() < 1e-6 && ((en * 30.0).round() - en * 30.0).abs() < 1e-6);
        }
    }

    #[test]
    fn loud_gaps_are_kept() {
        let words = vec![w("Go", 0.0, 0.4), w("yes", 5.0, 5.4)];
        let loud = |_: f64, _: f64| false;
        let e = EditList::build(&EditParams { words: &words, start: 0.0, end: 6.0, deleted: &[], remove_silence: true, fps: 30.0, quiet: &loud });
        assert_eq!(e.segs, vec![(0.0, 6.0)]);
    }

    #[test]
    fn deleted_words() {
        let words = vec![w("a", 0.0, 0.5), w("bad", 0.5, 1.0), w("c", 1.0, 1.5)];
        let q = |_: f64, _: f64| false;
        let e = EditList::build(&EditParams { words: &words, start: 0.0, end: 1.5, deleted: &[1], remove_silence: false, fps: 30.0, quiet: &q });
        assert!((e.duration() - 1.0).abs() < 0.05);
        assert_eq!(e.retime(&words, &[1]).len(), 2);
    }
}
