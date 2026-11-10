//! Prompt building and strict validation for the optional local model.
//!
//! The model never decides the result. It may (a) suggest the next middle
//! candidate, which [`crate::search::Search::propose_mid`] clamps, and (b)
//! write a short plain-language explanation of a result the algorithm already
//! computed. Every model output passes through this module before use.

use crate::search::Observation;
use serde::Deserialize;

/// Maximum characters kept from a model explanation.
pub const MAX_EXPLANATION_CHARS: usize = 700;

/// Context passed to prompt builders.
#[derive(Clone, Debug)]
pub struct AdviceContext<'a> {
    /// Game display name.
    pub game: &'a str,
    /// Mouse DPI.
    pub dpi: f64,
    /// Starting in-game sensitivity.
    pub start_sens: f64,
    /// Observations so far.
    pub observations: &'a [Observation],
    /// Range the next middle candidate must fall in (multipliers).
    pub bounds: (f64, f64),
}

fn obs_table(obs: &[Observation]) -> String {
    let mut s = String::from("round,multiplier,score,accuracy,speed,control,tracking,overshoot_bias\n");
    for o in obs {
        s.push_str(&format!(
            "{},{:.3},{:.1},{:.2},{:.2},{:.2},{:.2},{:+.2}\n",
            o.round + 1,
            o.mult,
            o.score.total,
            o.score.accuracy,
            o.score.speed,
            o.score.control,
            o.score.tracking,
            o.score.bias
        ));
    }
    s
}

/// System prompt shared by both tasks.
pub const SYSTEM_PROMPT: &str = "You are a careful assistant inside a mouse-sensitivity calibration tool for PC shooters. \
You analyse aim-test scores. Be concise and factual. Never invent numbers that are not in the data.";

/// Prompt asking for the next middle candidate as JSON.
pub fn next_candidate_prompt(ctx: &AdviceContext<'_>) -> String {
    format!(
        "/no_think\nGame: {game}. Mouse: {dpi:.0} DPI. Starting sensitivity: {sens}.\n\
Candidates are multipliers of the starting sensitivity. Higher score is better. \
Positive overshoot_bias means the player overshoots (too fast); negative means stopping short (too slow).\n\n{table}\n\
Pick the multiplier most likely to score highest next, between {lo:.3} and {hi:.3}. \
Reply with only JSON: {{\"next_multiplier\": <number>, \"reason\": \"<one short sentence>\"}}",
        game = ctx.game,
        dpi = ctx.dpi,
        sens = ctx.start_sens,
        table = obs_table(ctx.observations),
        lo = ctx.bounds.0,
        hi = ctx.bounds.1,
    )
}

/// Facts about a finished result, used for the explanation prompt and the built-in fallback.
#[derive(Clone, Debug)]
pub struct ResultFacts<'a> {
    /// Game display name.
    pub game: &'a str,
    /// Starting sensitivity.
    pub start_sens: f64,
    /// Recommended sensitivity (already rounded).
    pub recommended_sens: f64,
    /// cm/360 at the recommendation.
    pub cm360: f64,
    /// Starting cm/360.
    pub start_cm360: f64,
    /// Confidence 0–1.
    pub confidence: f64,
    /// All observations.
    pub observations: &'a [Observation],
}

/// Prompt asking for a plain-language explanation.
pub fn explanation_prompt(f: &ResultFacts<'_>) -> String {
    format!(
        "/no_think\nGame: {game}. The calibration algorithm already chose the result: sensitivity {rec} ({cm:.1} cm/360), \
starting from {start} ({scm:.1} cm/360). Confidence {conf:.0}%.\n\nScores per tested multiplier:\n{table}\n\
In at most 80 words of plain English, explain to the player why this sensitivity fits them, mentioning accuracy, speed, \
overshoot or tracking where the data supports it. Do not suggest a different number. Plain text only, no lists, no markdown.",
        game = f.game,
        rec = f.recommended_sens,
        cm = f.cm360,
        start = f.start_sens,
        scm = f.start_cm360,
        conf = f.confidence * 100.0,
        table = obs_table(f.observations),
    )
}

/// Remove `<think>…</think>` blocks (and an unterminated trailing one).
pub fn strip_thinking(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    loop {
        match rest.find("<think>") {
            Some(i) => {
                out.push_str(&rest[..i]);
                match rest[i..].find("</think>") {
                    Some(j) => rest = &rest[i + j + "</think>".len()..],
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

#[derive(Deserialize)]
struct RawAdvice {
    next_multiplier: f64,
    #[serde(default)]
    reason: String,
}

/// Validated next-candidate advice.
#[derive(Clone, Debug, PartialEq)]
pub struct Advice {
    /// Proposed multiplier, already clamped to the given bounds.
    pub next_mult: f64,
    /// Sanitised one-line reason.
    pub reason: String,
}

/// Parse the model's JSON reply. Returns `None` for anything malformed or non-finite.
pub fn parse_advice(raw: &str, bounds: (f64, f64)) -> Option<Advice> {
    let text = strip_thinking(raw);
    let start = text.find('{')?;
    let end = text[start..].find('}')? + start;
    let parsed: RawAdvice = serde_json::from_str(&text[start..=end]).ok()?;
    if !parsed.next_multiplier.is_finite() || parsed.next_multiplier <= 0.0 {
        return None;
    }
    let (lo, hi) = bounds;
    Some(Advice { next_mult: parsed.next_multiplier.clamp(lo, hi), reason: sanitize_explanation(&parsed.reason, 200) })
}

/// Clean model text for display: drop thinking, markdown symbols, links and
/// control characters, collapse whitespace and cap the length.
pub fn sanitize_explanation(raw: &str, max_chars: usize) -> String {
    let text = strip_thinking(raw);
    let mut words: Vec<&str> = Vec::new();
    for w in text.split_whitespace() {
        let lw = w.to_ascii_lowercase();
        if lw.starts_with("http://") || lw.starts_with("https://") || lw.starts_with("www.") {
            continue;
        }
        words.push(w);
    }
    let joined = words.join(" ");
    let cleaned: String = joined.chars().filter(|c| !c.is_control() && !matches!(c, '*' | '#' | '`' | '<' | '>' | '|')).collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    let cut: String = cleaned.chars().take(max_chars).collect();
    match cut.rfind(['.', '!', '?']) {
        Some(i) if i > max_chars / 2 => cut[..=i].to_string(),
        _ => format!("{}…", cut.trim_end()),
    }
}

/// Deterministic explanation used when the model is unavailable or its output is rejected.
pub fn fallback_explanation(f: &ResultFacts<'_>) -> String {
    let best = f.observations.iter().max_by(|a, b| a.score.total.partial_cmp(&b.score.total).unwrap_or(std::cmp::Ordering::Equal));
    let mut s = String::new();
    let change = f.recommended_sens / f.start_sens - 1.0;
    if change.abs() < 0.03 {
        s.push_str(&format!(
            "Your current {} sensitivity already scored at the top of the tested range, so it stays about the same. ",
            f.game
        ));
    } else if change > 0.0 {
        s.push_str(&format!("You scored best about {:.0}% faster than your current setting. ", change * 100.0));
    } else {
        s.push_str(&format!("You scored best about {:.0}% slower than your current setting. ", -change * 100.0));
    }
    if let Some(b) = best {
        if b.score.bias > 0.15 {
            s.push_str("Even at this setting you tended to overshoot a little; if it feels fast in game, go slightly lower. ");
        } else if b.score.bias < -0.15 {
            s.push_str("At this setting you still stopped short of some targets; if it feels slow in game, go slightly higher. ");
        }
        s.push_str(&format!(
            "At the best setting you hit {:.0}% of targets and stayed on the tracking target {:.0}% of the time. ",
            b.score.accuracy * 100.0,
            b.score.tracking * 100.0
        ));
    }
    s.push_str(&format!("That is {:.1} cm of mouse travel per full turn (was {:.1} cm).", f.cm360, f.start_cm360));
    if f.confidence < 0.5 {
        s.push_str(" Scores were close together, so a second run is worth doing.");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::Score;

    #[test]
    fn parses_clean_json() {
        let a = parse_advice("{\"next_multiplier\": 1.12, \"reason\": \"Peak is between mid and high.\"}", (0.8, 1.3)).unwrap();
        assert_eq!(a.next_mult, 1.12);
        assert_eq!(a.reason, "Peak is between mid and high.");
    }

    #[test]
    fn strips_thinking_and_clamps() {
        let raw = "<think>lots of {\"next_multiplier\": 9}</think> Sure! {\"next_multiplier\": 5.0}";
        assert_eq!(parse_advice(raw, (0.8, 1.3)).unwrap().next_mult, 1.3);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_advice("no json here", (0.8, 1.2)).is_none());
        assert!(parse_advice("{\"next_multiplier\": \"fast\"}", (0.8, 1.2)).is_none());
        assert!(parse_advice("{\"next_multiplier\": -3}", (0.8, 1.2)).is_none());
        assert!(parse_advice("<think>unterminated {\"next_multiplier\": 1.0}", (0.8, 1.2)).is_none());
    }

    #[test]
    fn sanitizes_text() {
        let s = sanitize_explanation("**Great** aim! see https://evil.example now\u{7}", 100);
        assert_eq!(s, "Great aim! see now");
        let long = "Word. ".repeat(200);
        assert!(sanitize_explanation(&long, 50).chars().count() <= 51);
    }

    #[test]
    fn fallback_mentions_direction() {
        let obs = [Observation { round: 0, mult: 1.2, score: Score { total: 70.0, accuracy: 0.8, tracking: 0.6, ..Default::default() } }];
        let f = ResultFacts {
            game: "Valorant",
            start_sens: 0.4,
            recommended_sens: 0.48,
            cm360: 30.0,
            start_cm360: 36.0,
            confidence: 0.8,
            observations: &obs,
        };
        let t = fallback_explanation(&f);
        assert!(t.contains("20% faster"), "{t}");
        assert!(t.contains("30.0 cm"));
        let p = explanation_prompt(&f);
        assert!(p.contains("0.48") && p.contains("/no_think"));
    }
}
