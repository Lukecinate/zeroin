//! Prompt building and strict validation for the optional local model.
//!
//! The model never decides the result and never proposes test positions. It
//! may only write a short plain-language explanation of a result the
//! algorithm already computed. Every model output passes through this module
//! before use, and text that states numbers the result does not contain, or
//! talks about confidence, is rejected in favour of the built-in summary.

use crate::search::Observation;

/// Maximum characters kept from a model explanation.
pub const MAX_EXPLANATION_CHARS: usize = 700;

fn obs_table(obs: &[Observation]) -> String {
    let mut s = String::from("block,multiplier,score,hit_rate,throughput_bits_s,tracking,stop_error_vs_typical\n");
    for o in obs {
        s.push_str(&format!(
            "{},{:.3},{:.1},{:.2},{:.2},{:.2},{:+.2}\n",
            o.round + 1,
            o.mult,
            o.score.v2,
            o.score.accuracy,
            o.score.throughput,
            o.score.tracking_v2,
            o.score.bias_corrected
        ));
    }
    s
}

/// System prompt.
pub const SYSTEM_PROMPT: &str = "You are a careful assistant inside a mouse-sensitivity calibration tool for PC shooters. \
You explain aim-test results. Be concise and factual. Never invent numbers that are not in the data.";

/// Facts about a finished result, used for the explanation prompt and the built-in fallback.
#[derive(Clone, Debug)]
pub struct ResultFacts<'a> {
    /// Game display name.
    pub game: &'a str,
    /// True when the advice is to keep the current sens.
    pub keep: bool,
    /// Starting sensitivity, formatted as shown to the player.
    pub start_text: String,
    /// Recommended sensitivity, formatted as shown.
    pub recommended_text: String,
    /// Starting sensitivity.
    pub start_sens: f64,
    /// Recommended sensitivity (already rounded).
    pub recommended_sens: f64,
    /// Range of about-equally-good sensitivities, formatted as shown.
    pub band_text: (String, String),
    /// Predicted gain of moving, in score points.
    pub gain_pts: f64,
    /// cm/360 at the recommendation.
    pub cm360: f64,
    /// Starting cm/360.
    pub start_cm360: f64,
    /// All valid observations.
    pub observations: &'a [Observation],
}

/// Prompt asking for a plain-language explanation.
pub fn explanation_prompt(f: &ResultFacts<'_>) -> String {
    let verdict = if f.keep {
        format!("keep the current sensitivity {} ({:.1} cm/360)", f.start_text, f.start_cm360)
    } else {
        format!("change from {} ({:.1} cm/360) to {} ({:.1} cm/360)", f.start_text, f.start_cm360, f.recommended_text, f.cm360)
    };
    format!(
        "/no_think\nGame: {game}. The calibration algorithm already decided: {verdict}. \
Settings from {lo} to {hi} scored about equally well. Multipliers are relative to the starting sensitivity; \
score is 0-100, higher is better; stop_error_vs_typical above 0 means flying past targets more than most people, below 0 stopping short.\n\n{table}\n\
In at most 80 words of plain English, explain this result to the player, mentioning hit rate, speed, tracking or overshoot only where the data supports it. \
Use only numbers that appear above. Do not suggest a different number and do not talk about confidence or probability. Plain text only, no lists, no markdown.",
        game = f.game,
        lo = f.band_text.0,
        hi = f.band_text.1,
        table = obs_table(f.observations),
    )
}

/// Check a model explanation against the facts. Rejects text that mentions
/// confidence, or contains a decimal number that is not one of the result's
/// own figures (so the model cannot state a different sensitivity).
pub fn explanation_is_consistent(text: &str, f: &ResultFacts<'_>) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("confiden") || lower.contains("probab") {
        return false;
    }
    let mut allowed: Vec<String> = vec![f.start_text.clone(), f.recommended_text.clone(), f.band_text.0.clone(), f.band_text.1.clone()];
    for v in [f.cm360, f.start_cm360] {
        allowed.push(format!("{v:.1}"));
        allowed.push(format!("{:.1}", half(v)));
        allowed.push(format!("{v:.0}"));
    }
    for o in f.observations {
        allowed.push(format!("{:.1}", o.score.v2));
        allowed.push(format!("{:.2}", o.score.throughput));
        allowed.push(format!("{:.3}", o.mult));
    }
    let norm = |t: &str| t.replace(',', ".").trim_end_matches('.').to_string();
    let allowed: Vec<String> = allowed.iter().map(|a| norm(a)).collect();
    let mut num = String::new();
    let chars: Vec<char> = text.chars().chain(std::iter::once(' ')).collect();
    for (i, c) in chars.iter().enumerate() {
        let joins = (*c == '.' || *c == ',') && !num.is_empty() && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        if c.is_ascii_digit() || joins {
            num.push(*c);
        } else if !num.is_empty() {
            let n = norm(&num);
            if n.contains('.') && !allowed.contains(&n) {
                return false;
            }
            num.clear();
        }
    }
    true
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

/// cm/360 is shown to the nearest half centimetre: finer digits suggest a precision the test does not have.
pub fn half(cm: f64) -> f64 {
    (cm * 2.0).round() / 2.0
}

/// Deterministic explanation used when the model is unavailable or its output is rejected.
pub fn fallback_explanation(f: &ResultFacts<'_>) -> String {
    let mut s = String::new();
    if f.keep {
        s.push_str(&format!(
            "Keep your current {} sensitivity. Moving would not clearly improve your scores: settings from {} to {} performed about the same for you. ",
            f.game, f.band_text.0, f.band_text.1
        ));
    } else {
        let change = f.recommended_sens / f.start_sens - 1.0;
        let dir = if change > 0.0 { "faster" } else { "slower" };
        s.push_str(&format!(
            "You scored better at a {dir} setting: {} instead of {}. Settings from {} to {} performed about equally well, so anything in that range is a sound choice. ",
            f.recommended_text, f.start_text, f.band_text.0, f.band_text.1
        ));
    }
    // Aim-bias advice uses the baseline-corrected stop error, and only when
    // it is consistent across most blocks.
    let n = f.observations.len();
    if n > 0 {
        let over = f.observations.iter().filter(|o| o.score.bias_corrected > 0.08).count();
        let under = f.observations.iter().filter(|o| o.score.bias_corrected < -0.08).count();
        if over * 3 >= n * 2 {
            s.push_str(
                "You tended to fly past targets more than most people do; if it feels fast in game, pick the lower end of the range. ",
            );
        } else if under * 3 >= n * 2 {
            s.push_str(
                "You tended to stop short of targets more than most people do; if it feels slow in game, pick the upper end of the range. ",
            );
        }
    }
    if f.keep {
        s.push_str(&format!("That is about {:.1} cm of mouse travel per full turn.", half(f.start_cm360)));
    } else {
        s.push_str(&format!("That is about {:.1} cm of mouse travel per full turn (was {:.1} cm).", half(f.cm360), half(f.start_cm360)));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::Score;

    fn facts(obs: &[Observation], keep: bool) -> ResultFacts<'_> {
        ResultFacts {
            game: "Valorant",
            keep,
            start_text: "0.4".into(),
            recommended_text: if keep { "0.4".into() } else { "0.48".into() },
            start_sens: 0.4,
            recommended_sens: if keep { 0.4 } else { 0.48 },
            band_text: ("0.42".into(), "0.53".into()),
            gain_pts: 2.5,
            cm360: if keep { 36.0 } else { 30.0 },
            start_cm360: 36.0,
            observations: obs,
        }
    }

    #[test]
    fn sanitizes_text() {
        let s = sanitize_explanation("**Great** aim! see https://evil.example now\u{7}", 100);
        assert_eq!(s, "Great aim! see now");
        let long = "Word. ".repeat(200);
        assert!(sanitize_explanation(&long, 50).chars().count() <= 51);
    }

    #[test]
    fn strips_thinking() {
        assert_eq!(strip_thinking("<think>x</think>Hi"), "Hi");
        assert_eq!(strip_thinking("Hi<think>unterminated"), "Hi");
    }

    #[test]
    fn fallback_mentions_direction_and_range() {
        let obs = [Observation { round: 0, mult: 1.2, score: Score { v2: 70.0, accuracy: 0.8, ..Default::default() } }];
        let f = facts(&obs, false);
        let t = fallback_explanation(&f);
        assert!(t.contains("faster") && t.contains("0.48") && t.contains("0.42 to 0.53"), "{t}");
        assert!(t.contains("30.0 cm"));
        assert!(!t.to_lowercase().contains("confiden"));
        let p = explanation_prompt(&f);
        assert!(p.contains("0.48") && p.contains("/no_think") && !p.contains("Confidence"));
        let k = fallback_explanation(&facts(&obs, true));
        assert!(k.starts_with("Keep your current Valorant sensitivity"), "{k}");
    }

    #[test]
    fn bias_advice_uses_the_corrected_value() {
        // Raw undershoot of 11 % is typical, so no advice.
        let typical = Score { v2: 60.0, bias: -0.11, bias_corrected: 0.0, ..Default::default() };
        let obs: Vec<Observation> = (0..6).map(|i| Observation { round: i, mult: 1.0, score: typical }).collect();
        let t = fallback_explanation(&facts(&obs, true));
        assert!(!t.contains("stop short") && !t.contains("fly past"), "{t}");
        let past = Score { bias_corrected: 0.15, ..typical };
        let obs: Vec<Observation> = (0..6).map(|i| Observation { round: i, mult: 1.0, score: past }).collect();
        assert!(fallback_explanation(&facts(&obs, true)).contains("fly past"));
    }

    #[test]
    fn rejects_inconsistent_model_text() {
        let obs = [Observation { round: 0, mult: 1.2, score: Score { v2: 70.0, ..Default::default() } }];
        let f = facts(&obs, false);
        assert!(explanation_is_consistent("Your best setting is 0.48, about 30 cm per turn. You hit 8 of 11 targets.", &f));
        assert!(explanation_is_consistent("Anything from 0,42 to 0,53 felt the same.", &f));
        assert!(!explanation_is_consistent("Try 0.55 instead.", &f));
        assert!(!explanation_is_consistent("We are 90% confident in 0.48.", &f));
    }
}
