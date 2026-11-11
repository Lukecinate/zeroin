//! Self-reported gear: mousepad and mouse feet.
//!
//! These values are context for the player's own history only. They are never
//! used by the search, the scoring or any recommendation.

use serde::{Deserialize, Serialize};

/// Maximum characters kept from the free-text mousepad name.
pub const MAX_PAD_NAME: usize = 60;

/// Mousepad surface material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadSurface {
    /// Woven cloth top.
    Cloth,
    /// Coated or hybrid weave.
    Hybrid,
    /// Glass.
    Glass,
    /// Hard plastic or aluminium.
    Hard,
    /// Anything else.
    Other,
}

impl PadSurface {
    /// All values in display order.
    pub const ALL: [PadSurface; 5] = [PadSurface::Cloth, PadSurface::Hybrid, PadSurface::Glass, PadSurface::Hard, PadSurface::Other];
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            PadSurface::Cloth => "Cloth",
            PadSurface::Hybrid => "Hybrid",
            PadSurface::Glass => "Glass",
            PadSurface::Hard => "Hard/plastic",
            PadSurface::Other => "Other",
        }
    }
}

/// Mousepad speed class as sold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadSpeed {
    /// Sold as a speed (low friction) surface.
    Speed,
    /// Sold as balanced.
    Balanced,
    /// Sold as a control (high friction) surface.
    Control,
}

impl PadSpeed {
    /// All values in display order.
    pub const ALL: [PadSpeed; 3] = [PadSpeed::Speed, PadSpeed::Balanced, PadSpeed::Control];
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            PadSpeed::Speed => "Speed",
            PadSpeed::Balanced => "Balanced",
            PadSpeed::Control => "Control",
        }
    }
}

/// Mouse feet (skates).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feet {
    /// Feet the mouse shipped with.
    Stock,
    /// Aftermarket PTFE skates.
    Ptfe,
    /// Glass.
    Glass,
    /// Ceramic skates.
    Ceramic,
    /// Anything else.
    Other,
}

impl Feet {
    /// All values in display order.
    pub const ALL: [Feet; 5] = [Feet::Stock, Feet::Ptfe, Feet::Glass, Feet::Ceramic, Feet::Other];
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Feet::Stock => "Stock",
            Feet::Ptfe => "PTFE (aftermarket)",
            Feet::Glass => "Glass",
            Feet::Ceramic => "Ceramic",
            Feet::Other => "Other",
        }
    }
}

/// Gear as entered by the player. Every field is optional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gear {
    /// Mousepad surface.
    pub pad_surface: Option<PadSurface>,
    /// Mousepad speed class.
    pub pad_speed: Option<PadSpeed>,
    /// Free-text brand and model, sanitised with [`sanitize_text`].
    pub pad_name: String,
    /// Mouse feet.
    pub feet: Option<Feet>,
}

/// Remove control and formatting characters, collapse whitespace, trim, and
/// keep at most `max` characters.
pub fn sanitize_text(s: &str, max: usize) -> String {
    let cleaned: String =
        s.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).filter(|c| !c.is_control() && !is_format_char(*c)).collect();
    let collapsed = cleaned.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    collapsed.chars().take(max).collect::<String>().trim_end().to_string()
}

/// Bidirectional overrides, zero-width and similar invisible characters.
fn is_format_char(c: char) -> bool {
    matches!(c as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x2069 | 0xFEFF | 0xFFF9..=0xFFFB)
}

impl Gear {
    /// Copy with the free text sanitised.
    pub fn sanitized(&self) -> Gear {
        Gear { pad_name: sanitize_text(&self.pad_name, MAX_PAD_NAME), ..self.clone() }
    }

    /// Nothing entered.
    pub fn is_empty(&self) -> bool {
        self.pad_surface.is_none() && self.pad_speed.is_none() && self.pad_name.trim().is_empty() && self.feet.is_none()
    }

    /// Mousepad description used for grouping and filtering, or `None` when
    /// no mousepad detail was entered. The name wins over the categories.
    pub fn pad_label(&self) -> Option<String> {
        let name = sanitize_text(&self.pad_name, MAX_PAD_NAME);
        let cats: Vec<&str> = [self.pad_surface.map(|s| s.label()), self.pad_speed.map(|s| s.label())].into_iter().flatten().collect();
        match (name.is_empty(), cats.is_empty()) {
            (true, true) => None,
            (true, false) => Some(cats.join(" · ")),
            (false, true) => Some(name),
            (false, false) => Some(format!("{name} ({})", cats.join(" · "))),
        }
    }

    /// One-line summary, for example "Cloth · Control pad, PTFE (aftermarket) feet".
    pub fn summary(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(p) = self.pad_label() {
            parts.push(format!("Pad: {p}"));
        }
        if let Some(f) = self.feet {
            parts.push(format!("Feet: {}", f.label()));
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_controls_and_limits_length() {
        assert_eq!(sanitize_text("  Artisan\tZero\n Mid  ", 60), "Artisan Zero Mid");
        assert_eq!(sanitize_text("a\u{0007}b\u{202E}c\u{200B}d", 60), "abcd");
        let long = "x".repeat(200);
        assert_eq!(sanitize_text(&long, MAX_PAD_NAME).chars().count(), MAX_PAD_NAME);
        assert_eq!(sanitize_text("ééé", 2), "éé");
        assert_eq!(sanitize_text("\u{0000}\u{001b}[31m", 60), "[31m");
    }

    #[test]
    fn labels_and_empty() {
        let mut g = Gear::default();
        assert!(g.is_empty());
        assert_eq!(g.pad_label(), None);
        assert_eq!(g.summary(), None);
        g.pad_surface = Some(PadSurface::Cloth);
        g.pad_speed = Some(PadSpeed::Control);
        assert_eq!(g.pad_label().as_deref(), Some("Cloth · Control"));
        g.pad_name = " Zero\u{0}  Mid ".into();
        assert_eq!(g.pad_label().as_deref(), Some("Zero Mid (Cloth · Control)"));
        g.feet = Some(Feet::Ptfe);
        assert_eq!(g.summary().as_deref(), Some("Pad: Zero Mid (Cloth · Control) · Feet: PTFE (aftermarket)"));
        assert_eq!(g.sanitized().pad_name, "Zero Mid");
    }

    #[test]
    fn search_queries_are_sanitised() {
        assert_eq!(search_query("Artisan Zero Mid").as_deref(), Some("Artisan Zero Mid mousepad specs"));
        assert_eq!(search_query("  <script>alert(1)</script> ").as_deref(), Some("scriptalert(1)/script mousepad specs"));
        assert_eq!(search_query("a&b=c#d?e%f").as_deref(), Some("abcdef mousepad specs"));
        assert_eq!(search_query("\u{202E}\u{0007}"), None);
        assert_eq!(search_query("---"), None);
        assert!(search_query(&"y".repeat(500)).unwrap().len() <= MAX_PAD_NAME + " mousepad specs".len());
        let u = search_url("Zero ½ Mid").unwrap();
        assert!(u.starts_with("https://duckduckgo.com/?q="));
        assert_eq!(u, "https://duckduckgo.com/?q=Zero+%C2%BD+Mid+mousepad+specs");
        assert!(!u[SEARCH_BASE.len()..].contains(['&', '#', '?', ' ', '<', '>', '"']));
    }

    #[test]
    fn serde_round_trip_and_defaults() {
        let g = Gear { pad_surface: Some(PadSurface::Hard), pad_speed: None, pad_name: "X".into(), feet: Some(Feet::Ceramic) };
        let s = serde_json::to_string(&g).unwrap();
        assert!(s.contains("\"hard\"") && s.contains("\"ceramic\""));
        assert_eq!(serde_json::from_str::<Gear>(&s).unwrap(), g);
        assert_eq!(serde_json::from_str::<Gear>("{}").unwrap(), Gear::default());
    }
}

// ---------------------------------------------------------------------------
// Browser search for a mousepad name

/// Search page opened in the player's browser. ZeroIn itself makes no request.
pub const SEARCH_BASE: &str = "https://duckduckgo.com/?q=";

/// Characters kept in a search query: letters, digits, spaces and a few
/// punctuation marks that appear in product names.
fn query_char_ok(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, ' ' | '-' | '.' | '+' | '/' | '(' | ')' | '\'')
}

/// Search query for a typed mousepad name: sanitised, limited to
/// [`MAX_PAD_NAME`] characters, with "mousepad specs" appended. `None` when
/// nothing searchable is left.
pub fn search_query(name: &str) -> Option<String> {
    let clean: String = sanitize_text(name, MAX_PAD_NAME).chars().filter(|c| query_char_ok(*c)).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    (clean.chars().any(|c| c.is_alphanumeric())).then(|| format!("{clean} mousepad specs"))
}

/// Percent-encode a query for a URL (UTF-8, unreserved characters kept).
pub fn url_encode(q: &str) -> String {
    let mut out = String::new();
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Full HTTPS search URL for a typed mousepad name.
pub fn search_url(name: &str) -> Option<String> {
    search_query(name).map(|q| format!("{SEARCH_BASE}{}", url_encode(&q)))
}

// ---------------------------------------------------------------------------
// Gear advisory
//
// Basis: ZeroIn research note 17 (public CS2 pro data from CSDB.gg, n = 816;
// Richard 2000; Boudaoud et al. 2022/2023). No reliable sensitivity shift by
// pad category was found, glass had n = 5 and feet n = 0, so no numeric bias
// is applied. The advisory is text only and never changes a measured number.

/// One cell of the pad/feet bias table: a target group compared with a
/// reference group in one game. Values are aggregates from public data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiasCell {
    /// Game id the data comes from.
    pub game: &'static str,
    /// Target group: surface (if part of the group).
    pub surface: Option<PadSurface>,
    /// Target group: speed class (if part of the group).
    pub speed: Option<PadSpeed>,
    /// Target group: feet (if part of the group).
    pub feet: Option<Feet>,
    /// Players in the target group.
    pub n: u32,
    /// Players in the reference group.
    pub n_ref: u32,
    /// Median cm/360 shift of the target group as a fraction of the reference median.
    pub shift_frac: f64,
    /// 95 % confidence interval of that shift (fractions).
    pub ci_frac: (f64, f64),
    /// Citation.
    pub source: &'static str,
}

/// Minimum players per group before a cell can drive a bias.
pub const MIN_CELL_N: u32 = 50;
/// Largest bias ever applied, as a fraction of cm/360.
pub const MAX_BIAS_FRAC: f64 = 0.10;

impl BiasCell {
    /// Enough players in both groups and a confidence interval that excludes zero.
    pub fn qualifies(&self) -> bool {
        self.n >= MIN_CELL_N && self.n_ref >= MIN_CELL_N && (self.ci_frac.0 > 0.0 || self.ci_frac.1 < 0.0) && self.shift_frac.is_finite()
    }

    fn matches(&self, g: &Gear, game: &str) -> bool {
        let any = self.surface.is_some() || self.speed.is_some() || self.feet.is_some();
        any && self.game == game
            && self.surface.is_none_or(|s| g.pad_surface == Some(s))
            && self.speed.is_none_or(|s| g.pad_speed == Some(s))
            && self.feet.is_none_or(|f| g.feet == Some(f))
    }
}

const CSDB_PADS: &str = "CSDB.gg mousepad pages, \"Most Used Mousepads in CS2 (Pro Usage 2026)\", data as of 9 Nov 2026, accessed 11 Nov 2026. https://csdb.gg/gear/rankings/mousepad/";

/// Bias table. Reference group: cloth control pads in CS2 (n = 451, median
/// 52.0 cm/360). None of these cells qualifies, so no bias is applied.
pub const BIAS_CELLS: &[BiasCell] = &[
    BiasCell {
        game: "cs2",
        surface: Some(PadSurface::Cloth),
        speed: Some(PadSpeed::Speed),
        feet: None,
        n: 87,
        n_ref: 451,
        shift_frac: 0.0,
        ci_frac: (-0.047, 0.099),
        source: CSDB_PADS,
    },
    BiasCell {
        game: "cs2",
        surface: None,
        speed: Some(PadSpeed::Balanced),
        feet: None,
        n: 225,
        n_ref: 451,
        shift_frac: 0.0,
        ci_frac: (-0.038, 0.029),
        source: CSDB_PADS,
    },
    BiasCell {
        game: "cs2",
        surface: Some(PadSurface::Glass),
        speed: None,
        feet: None,
        n: 5,
        n_ref: 451,
        shift_frac: 0.177,
        ci_frac: (-0.6, 1.9),
        source: CSDB_PADS,
    },
];

/// Bias (fraction of cm/360, capped at ±[`MAX_BIAS_FRAC`]) from the first
/// matching cell that qualifies. `None` for every cell in the shipped table.
pub fn qualified_bias_with(cells: &[BiasCell], gear: &Gear, game: &str) -> Option<f64> {
    cells.iter().find(|c| c.matches(gear, game) && c.qualifies()).map(|c| c.shift_frac.clamp(-MAX_BIAS_FRAC, MAX_BIAS_FRAC))
}

/// [`qualified_bias_with`] on the shipped table.
pub fn qualified_bias(gear: &Gear, game: &str) -> Option<f64> {
    qualified_bias_with(BIAS_CELLS, gear, game)
}

/// Advisory text (English).
pub const ADVICE_EN: &str = "Your sensitivity comes from this test, not from your mousepad. A faster surface (a speed cloth pad or a glass pad) has less friction to help you stop, so the same sens can feel harder to control at first. Some players then lower their sens a little; others keep it or even raise it. Public pro data does not show a reliable difference in the sens people actually use on control, speed, or glass pads, and there is not enough data for mouse feet, so ZeroIn does not change the number. If stopping feels difficult on this surface, you can try about 5–10% more cm/360 (a slightly slower sens) — that still sits in a typical equal-performance range — and run the test again.";

/// Advisory text for a control cloth pad (English).
pub const ADVICE_CONTROL_EN: &str = "Your sensitivity comes from this test, not from your mousepad. If you later switch to a faster pad (a speed cloth or a glass pad), it has less friction to help you stop, so the same sens can feel harder to control at first. Some players then lower their sens a little; others keep it or even raise it. Public pro data does not show a reliable difference in the sens people actually use on control, speed, or glass pads, and there is not enough data for mouse feet, so ZeroIn does not change the number. If stopping feels difficult on a new surface, you can try about 5–10% more cm/360 (a slightly slower sens) — that still sits in a typical equal-performance range — and run the test again.";

/// Advisory text (Indonesian).
pub const ADVICE_ID: &str = "Sensitivitasmu berasal dari tes ini, bukan dari mousepad. Permukaan yang lebih cepat (mousepad kain tipe speed atau glasspad) punya gesekan yang lebih kecil untuk menahan laju mouse, jadi sens yang sama bisa terasa lebih sulit dikontrol pada awalnya. Sebagian pemain lalu menurunkan sens sedikit; yang lain tetap atau justru menaikkan. Data publik dari pro tidak menunjukkan perbedaan yang andal pada sens yang benar-benar dipakai di pad control, speed, atau kaca, dan data mouse feet masih kurang, jadi ZeroIn tidak mengubah angkanya. Jika mouse terasa sulit berhenti di permukaan ini, kamu bisa mencoba cm/360 sekitar 5–10% lebih besar (sens sedikit lebih lambat) — itu masih di rentang performa yang biasanya setara — lalu tes ulang.";

/// "Why we say this" points with citations.
pub const ADVICE_WHY: &[&str] = &[
    "Same sens, different feel: lower kinetic friction reduces passive braking. Sources: Richard, C. (2000), Fitts targeting with real and rendered friction (ASME IMECE; Stanford thesis \"On the Identification and Haptic Display of Friction\"); Corsair, \"Glass Mouse Pad vs Cloth\" (updated 26 Mar 2026).",
    "Equal-performance band for first-person aiming: about 20–80 cm/360. Source: Boudaoud, Spjut, Kim et al., CHI 2022 / IEEE Transactions on Graphics 2023.",
    "CS2 professionals, public configs: median 52 cm/360 on cloth control, balanced and speed pads alike; glass n = 5, not usable. Source: CSDB.gg mousepad pages and \"What Sensitivity Do CS2 Pros Use? eDPI Distribution by Role\", data as of 9 Nov 2026, accessed 11 Nov 2026. https://csdb.gg/data/pro-sensitivity/",
];

/// Optional experiment: multipliers on the measured cm/360 (5–10 % more).
pub const TRY_SLOWER_CM360: (f64, f64) = (1.05, 1.10);

/// Advisory for the result screen.
#[derive(Clone, Debug, PartialEq)]
pub struct GearAdvice {
    /// Main text.
    pub text: &'static str,
    /// Citations for the "Why we say this" section.
    pub why: &'static [&'static str],
    /// Optional experiment as cm/360 multipliers.
    pub try_slower: (f64, f64),
    /// Qualified data-driven bias, if any cell qualifies (none today).
    pub bias: Option<f64>,
}

/// Advisory when a pad or feet value is present; `None` otherwise. Pure: it
/// reads the gear and returns text, and has no access to the measured result.
pub fn advice(gear: &Gear, game: &str) -> Option<GearAdvice> {
    let has_pad = gear.pad_surface.is_some() || gear.pad_speed.is_some() || !sanitize_text(&gear.pad_name, MAX_PAD_NAME).is_empty();
    if !has_pad && gear.feet.is_none() {
        return None;
    }
    let control = gear.pad_speed == Some(PadSpeed::Control);
    Some(GearAdvice {
        text: if control { ADVICE_CONTROL_EN } else { ADVICE_EN },
        why: ADVICE_WHY,
        try_slower: TRY_SLOWER_CM360,
        bias: qualified_bias(gear, game),
    })
}

#[cfg(test)]
mod advisory_tests {
    use super::*;

    #[test]
    fn no_advice_without_gear() {
        assert!(advice(&Gear::default(), "cs2").is_none());
        assert!(advice(&Gear { pad_name: " \u{7} ".into(), ..Default::default() }, "cs2").is_none());
    }

    #[test]
    fn advice_text_and_control_variant() {
        let a = advice(&Gear { feet: Some(Feet::Glass), ..Default::default() }, "valorant").unwrap();
        assert_eq!(a.text, ADVICE_EN);
        assert_eq!(a.try_slower, (1.05, 1.10));
        assert!(a.why.iter().any(|w| w.contains("CSDB.gg") && w.contains("accessed 11 Nov 2026")));
        let c = advice(&Gear { pad_surface: Some(PadSurface::Cloth), pad_speed: Some(PadSpeed::Control), ..Default::default() }, "cs2")
            .unwrap();
        assert_eq!(c.text, ADVICE_CONTROL_EN);
        assert!(!ADVICE_ID.is_empty());
    }

    #[test]
    fn shipped_cells_do_not_qualify() {
        assert!(BIAS_CELLS.iter().all(|c| !c.qualifies()));
        for s in PadSurface::ALL {
            for p in PadSpeed::ALL {
                let g = Gear { pad_surface: Some(s), pad_speed: Some(p), feet: Some(Feet::Glass), ..Default::default() };
                assert_eq!(qualified_bias(&g, "cs2"), None);
                assert_eq!(advice(&g, "cs2").unwrap().bias, None);
            }
        }
    }

    #[test]
    fn a_qualifying_cell_is_capped() {
        let cell = BiasCell {
            game: "cs2",
            surface: Some(PadSurface::Glass),
            speed: None,
            feet: None,
            n: 80,
            n_ref: 400,
            shift_frac: 0.25,
            ci_frac: (0.05, 0.4),
            source: "test",
        };
        let g = Gear { pad_surface: Some(PadSurface::Glass), ..Default::default() };
        assert!(cell.qualifies());
        assert_eq!(qualified_bias_with(&[cell], &g, "cs2"), Some(MAX_BIAS_FRAC));
        assert_eq!(qualified_bias_with(&[cell], &g, "valorant"), None);
        let small = BiasCell { n: 49, ..cell };
        assert_eq!(qualified_bias_with(&[small], &g, "cs2"), None);
        let ci_zero = BiasCell { ci_frac: (-0.01, 0.4), ..cell };
        assert_eq!(qualified_bias_with(&[ci_zero], &g, "cs2"), None);
    }

    #[test]
    fn advisory_never_changes_the_measured_numbers() {
        // `advice` only sees the gear and the game id; it cannot reach the
        // measured result. Check the gear is untouched too.
        let g = Gear { pad_speed: Some(PadSpeed::Speed), feet: Some(Feet::Glass), ..Default::default() };
        let before = g.clone();
        let _ = advice(&g, "cs2");
        assert_eq!(g, before);
    }
}
