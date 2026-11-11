//! Local history and settings files (JSON, written atomically).
//!
//! On Windows the app stores them in `%APPDATA%\ZeroIn`. Nothing is uploaded.

use crate::bayes::{BayesSearch, Mode};
use crate::drills::{DrillPlan, Preset};
use crate::games::GameProfile;
use crate::gear::Gear;
use crate::search::{Observation, RecommendationMethod};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io;
use std::path::Path;

/// Current history schema version.
pub const HISTORY_VERSION: u32 = 1;
/// Oldest sessions are dropped beyond this count.
pub const MAX_SESSIONS: usize = 500;

/// Windows mouse settings at the time of a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsMouse {
    /// Pointer speed slider, 1–20.
    pub pointer_speed: u32,
    /// "Enhance pointer precision" (acceleration).
    pub enhance_pointer_precision: bool,
}

/// Where the explanation text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplanationSource {
    /// The bundled local model.
    LocalModel,
    /// Built-in template.
    BuiltIn,
}

/// One finished calibration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Unique id (creation time in ms, as text).
    pub id: String,
    /// Unix time in seconds.
    pub created_unix: u64,
    /// Game tested.
    pub game: GameProfile,
    /// Mouse DPI.
    pub dpi: f64,
    /// Starting sensitivity.
    pub start_sens: f64,
    /// Recommended sensitivity (rounded to the game's precision).
    pub recommended_sens: f64,
    /// cm/360 at the recommendation.
    pub cm360: f64,
    /// 0.1 only: confidence 0–1. Not computed or shown since 0.2.
    #[serde(default)]
    pub confidence: f64,
    /// 0.2: the advice was to keep the starting sensitivity.
    #[serde(default)]
    pub keep: bool,
    /// 0.2: range of about-equally-good sensitivities (low, high).
    #[serde(default)]
    pub band: Option<(f64, f64)>,
    /// 0.2: predicted gain of moving, in score points.
    #[serde(default)]
    pub gain_pts: Option<f64>,
    /// 0.2: decision mode used.
    #[serde(default)]
    pub mode: Option<Mode>,
    /// How the recommendation was derived.
    pub method: RecommendationMethod,
    /// All scored candidates.
    pub observations: Vec<Observation>,
    /// Explanation shown on the result screen.
    pub explanation: String,
    /// Source of the explanation.
    pub explanation_source: ExplanationSource,
    /// 0.1 only: rounds where the local model's suggestion was used for the middle candidate.
    #[serde(default)]
    pub model_advised_rounds: Vec<u32>,
    /// Windows mouse settings, when readable.
    pub windows_mouse: Option<WindowsMouse>,
    /// Measured polling rate, if measured.
    pub polling_hz: Option<f64>,
    /// 0.2.1: self-reported mousepad and feet. Context only; never used in the result.
    #[serde(default)]
    pub gear: Option<Gear>,
    /// 0.2.1: whether Windows pointer settings were neutral for the test.
    #[serde(default)]
    pub pointer_during_test: Option<crate::winmouse::PointerDuringTest>,
    /// App version that produced the session.
    pub app_version: String,
}

/// The history file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct History {
    /// Schema version.
    pub version: u32,
    /// Sessions, oldest first.
    pub sessions: Vec<Session>,
}

impl Default for History {
    fn default() -> Self {
        History { version: HISTORY_VERSION, sessions: Vec::new() }
    }
}

impl History {
    /// Append a session, trimming the oldest beyond [`MAX_SESSIONS`].
    pub fn push(&mut self, s: Session) {
        self.sessions.push(s);
        if self.sessions.len() > MAX_SESSIONS {
            let extra = self.sessions.len() - MAX_SESSIONS;
            self.sessions.drain(..extra);
        }
    }

    /// Remove a session by id. Returns true if removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.sessions.len();
        self.sessions.retain(|s| s.id != id);
        n != self.sessions.len()
    }
}

/// Results grouped by self-reported mousepad, for comparison in History.
#[derive(Clone, Debug, PartialEq)]
pub struct PadSummary {
    /// Mousepad label ([`Gear::pad_label`]); `None` groups results without one.
    pub label: Option<String>,
    /// Number of results.
    pub count: usize,
    /// Median recommended cm/360.
    pub median_cm360: f64,
    /// Lowest and highest recommended cm/360.
    pub min_cm360: f64,
    /// Highest recommended cm/360.
    pub max_cm360: f64,
}

/// Mousepad label of a session, if any was entered.
pub fn pad_label(s: &Session) -> Option<String> {
    s.gear.as_ref().and_then(|g| g.pad_label())
}

/// Group sessions by mousepad label (case-insensitive), most results first.
/// The "no mousepad entered" group, if any, comes last.
pub fn by_pad(sessions: &[Session]) -> Vec<PadSummary> {
    let mut groups: Vec<(Option<String>, Vec<f64>)> = Vec::new();
    for s in sessions {
        let label = pad_label(s);
        let key = label.as_ref().map(|l| l.to_lowercase());
        match groups.iter_mut().find(|(l, _)| l.as_ref().map(|x| x.to_lowercase()) == key) {
            Some((_, v)) => v.push(s.cm360),
            None => groups.push((label, vec![s.cm360])),
        }
    }
    let mut out: Vec<PadSummary> = groups
        .into_iter()
        .map(|(label, mut v)| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = v.len();
            let median = if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) };
            PadSummary { label, count: n, median_cm360: median, min_cm360: v[0], max_cm360: v[n - 1] }
        })
        .collect();
    out.sort_by(|a, b| a.label.is_none().cmp(&b.label.is_none()).then(b.count.cmp(&a.count)));
    out
}

/// Test length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestLength {
    /// 6 blocks (3 bracket + 3 adaptive), about 2.5 minutes.
    Quick,
    /// 9 blocks, about 4 minutes.
    Standard,
}

impl TestLength {
    /// Planned valid blocks.
    pub fn blocks(self) -> u32 {
        match self {
            TestLength::Quick => 6,
            TestLength::Standard => 9,
        }
    }
}

/// User settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Use the local model for advice and explanations when available.
    pub ai_enabled: bool,
    /// Haptic pulses on supported mice.
    pub haptics_enabled: bool,
    /// Read analog key depth from Wooting keyboards when the SDK is installed.
    pub analog_keyboard_enabled: bool,
    /// Test length.
    pub test_length: TestLength,
    /// 0.1 only: search rounds. Ignored since 0.2.
    pub rounds: u32,
    /// How readily a change is recommended.
    pub mode: Mode,
    /// Reduce interface motion.
    pub reduce_motion: bool,
    /// Last used game id.
    pub last_game: Option<String>,
    /// Last used DPI.
    pub last_dpi: Option<f64>,
    /// Last entered gear, offered again in Setup.
    pub gear: Gear,
    /// Opt-in: offer a browser search for the typed mousepad name. Off by default.
    pub gear_lookup: bool,
    /// Temporary neutral Windows pointer settings during tests:
    /// `None` asks before each test, `Some(true)` always, `Some(false)` never.
    pub temp_mouse: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ai_enabled: true,
            haptics_enabled: false,
            analog_keyboard_enabled: true,
            test_length: TestLength::Standard,
            rounds: 4,
            mode: Mode::Balanced,
            reduce_motion: false,
            last_game: None,
            last_dpi: None,
            gear: Gear::default(),
            gear_lookup: false,
            temp_mouse: None,
        }
    }
}

/// Current saved-test schema version.
pub const SAVED_TEST_VERSION: u32 = 1;
/// A saved test older than this is not offered for resuming.
pub const RESUME_MAX_AGE_S: u64 = 24 * 3600;
/// Re-runs allowed per test for invalid blocks.
pub const MAX_RERUNS: u32 = 2;

/// An unfinished test, saved after every block so it can be resumed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedTest {
    /// Schema version.
    pub version: u32,
    /// Unix time the test started.
    pub created_unix: u64,
    /// Unix time of the last save.
    pub saved_unix: u64,
    /// Identity of the setup inputs (game, DPI, sens); resuming requires a match.
    pub fingerprint: String,
    /// Game tested.
    pub game: GameProfile,
    /// Mouse DPI.
    pub dpi: f64,
    /// Starting sensitivity.
    pub start_sens: f64,
    /// Target geometry preset.
    pub preset: Preset,
    /// Block plan.
    pub plan: DrillPlan,
    /// Seed of the paired target set.
    pub pair_seed: u64,
    /// Search state with every valid block so far.
    pub search: BayesSearch,
    /// Invalid blocks so far (each one was re-run).
    pub invalid_blocks: u32,
    /// Seconds each finished block took, for the time estimate.
    pub block_seconds: Vec<f64>,
}

impl SavedTest {
    /// True when the file is a version this build understands and recent enough to resume.
    pub fn is_resumable(&self, now_unix: u64) -> bool {
        self.version == SAVED_TEST_VERSION
            && now_unix.saturating_sub(self.saved_unix) <= RESUME_MAX_AGE_S
            && !self.search.is_done()
            && self.search.blocks_done() > 0
    }
}

/// Load JSON from `path`. A missing file yields `T::default()`.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

/// Write JSON atomically: write a temporary sibling, then rename over the target.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::{by_id, GameProfile};

    fn tmpdir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "zeroin-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            created_unix: 1,
            game: GameProfile::from_game(by_id("cs2").unwrap()),
            dpi: 800.0,
            start_sens: 1.0,
            recommended_sens: 1.1,
            cm360: 47.2,
            confidence: 0.7,
            keep: false,
            band: Some((1.0, 1.2)),
            gain_pts: Some(1.5),
            mode: Some(Mode::Balanced),
            method: RecommendationMethod::QuadraticFit,
            observations: vec![],
            explanation: "x".into(),
            explanation_source: ExplanationSource::BuiltIn,
            model_advised_rounds: vec![],
            windows_mouse: Some(WindowsMouse { pointer_speed: 10, enhance_pointer_precision: false }),
            polling_hz: Some(1000.0),
            gear: None,
            pointer_during_test: None,
            app_version: "0.1.0".into(),
        }
    }

    #[test]
    fn round_trip_and_missing_file() {
        let d = tmpdir();
        let p = d.join("history.json");
        let h: History = load_json(&p).unwrap();
        assert!(h.sessions.is_empty());
        let mut h = History::default();
        h.push(session("a"));
        save_json(&p, &h).unwrap();
        let back: History = load_json(&p).unwrap();
        assert_eq!(back, h);
        assert!(!p.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn corrupt_file_is_an_error_not_a_wipe() {
        let d = tmpdir();
        let p = d.join("history.json");
        std::fs::write(&p, b"{not json").unwrap();
        assert!(load_json::<History>(&p).is_err());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn trims_and_removes() {
        let mut h = History::default();
        for i in 0..(MAX_SESSIONS + 3) {
            h.push(session(&i.to_string()));
        }
        assert_eq!(h.sessions.len(), MAX_SESSIONS);
        assert_eq!(h.sessions[0].id, "3");
        assert!(h.remove("3"));
        assert!(!h.remove("nope"));
    }

    #[test]
    fn reads_a_0_1_session() {
        let mut v = serde_json::to_value(session("old")).unwrap();
        for k in ["keep", "band", "gain_pts", "mode"] {
            v.as_object_mut().unwrap().remove(k);
        }
        let s: Session = serde_json::from_value(v).unwrap();
        assert!(!s.keep && s.band.is_none());
        assert!(s.gear.is_none());
    }

    #[test]
    fn groups_by_pad() {
        use crate::gear::{PadSpeed, PadSurface};
        let mk = |id: &str, cm: f64, gear: Option<Gear>| Session { cm360: cm, gear, ..session(id) };
        let cloth = Gear { pad_surface: Some(PadSurface::Cloth), pad_speed: Some(PadSpeed::Control), ..Default::default() };
        let glass = Gear { pad_name: "Glass Pad X".into(), ..Default::default() };
        let glass_lower = Gear { pad_name: "glass pad x".into(), ..Default::default() };
        let feet_only = Gear { feet: Some(crate::gear::Feet::Ptfe), ..Default::default() };
        let v = vec![
            mk("1", 40.0, Some(cloth.clone())),
            mk("2", 44.0, Some(cloth.clone())),
            mk("3", 30.0, Some(glass)),
            mk("4", 50.0, None),
            mk("5", 42.0, Some(cloth)),
            mk("6", 34.0, Some(glass_lower)),
            mk("7", 51.0, Some(feet_only)),
        ];
        let g = by_pad(&v);
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].label.as_deref(), Some("Cloth · Control"));
        assert_eq!((g[0].count, g[0].median_cm360, g[0].min_cm360, g[0].max_cm360), (3, 42.0, 40.0, 44.0));
        assert_eq!(g[1].label.as_deref(), Some("Glass Pad X"));
        assert_eq!(g[1].median_cm360, 32.0);
        assert_eq!(g[2].label, None);
        assert_eq!(g[2].count, 2);
    }

    #[test]
    fn gear_round_trips_and_0_2_0_files_load() {
        use crate::gear::{Feet, PadSpeed, PadSurface};
        let mut s = session("g");
        s.gear = Some(Gear {
            pad_surface: Some(PadSurface::Glass),
            pad_speed: Some(PadSpeed::Speed),
            pad_name: "Pad".into(),
            feet: Some(Feet::Glass),
        });
        let mut h = History::default();
        h.push(s.clone());
        let back: History = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back.sessions[0].gear, s.gear);
        // A 0.2.0 file has no gear key at all.
        let mut v = serde_json::to_value(session("old")).unwrap();
        v.as_object_mut().unwrap().remove("gear");
        let old: Session = serde_json::from_value(v).unwrap();
        assert_eq!(old.gear, None);
        assert_eq!(old.pointer_during_test, None);
        let st: Settings = serde_json::from_str("{\"mode\": \"explorer\"}").unwrap();
        assert!(st.gear.is_empty());
        assert!(!st.gear_lookup, "gear lookup is off unless turned on");
        assert_eq!(st.temp_mouse, None, "older settings ask before the test");
        let st2: Settings = serde_json::from_str("{\"gear\": {\"feet\": \"ptfe\"}}").unwrap();
        assert_eq!(st2.gear.feet, Some(Feet::Ptfe));
    }

    #[test]
    fn saved_test_round_trip_and_age() {
        use crate::bayes::BayesConfig;
        use crate::scoring::Score;
        let mut search = BayesSearch::new(BayesConfig::new(6, Mode::Balanced, 0.0, 0.4));
        search.record(1.0, Score { v2: 50.0, ..Default::default() });
        let t = SavedTest {
            version: SAVED_TEST_VERSION,
            created_unix: 100,
            saved_unix: 200,
            fingerprint: "fp".into(),
            game: GameProfile::from_game(by_id("cs2").unwrap()),
            dpi: 800.0,
            start_sens: 1.0,
            preset: Preset::Tactical,
            plan: DrillPlan::for_preset(Preset::Tactical),
            pair_seed: 42,
            search,
            invalid_blocks: 1,
            block_seconds: vec![31.0],
        };
        let d = tmpdir();
        let p = d.join("session.json");
        save_json(&p, &t).unwrap();
        let back: Option<SavedTest> = load_json(&p).unwrap();
        let back = back.unwrap();
        assert_eq!(back.search.blocks_done(), 1);
        assert_eq!(back.pair_seed, 42);
        assert!(back.is_resumable(300));
        assert!(!back.is_resumable(200 + RESUME_MAX_AGE_S + 1));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str("{\"rounds\": 3}").unwrap();
        assert_eq!(s.rounds, 3);
        assert!(s.ai_enabled);
        assert_eq!(s.mode, Mode::Balanced);
    }
}
