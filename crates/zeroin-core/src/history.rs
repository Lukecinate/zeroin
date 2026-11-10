//! Local history and settings files (JSON, written atomically).
//!
//! On Windows the app stores them in `%APPDATA%\ZeroIn`. Nothing is uploaded.

use crate::games::GameProfile;
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
    /// Confidence 0–1.
    pub confidence: f64,
    /// How the recommendation was derived.
    pub method: RecommendationMethod,
    /// All scored candidates.
    pub observations: Vec<Observation>,
    /// Explanation shown on the result screen.
    pub explanation: String,
    /// Source of the explanation.
    pub explanation_source: ExplanationSource,
    /// Rounds where the local model's suggestion was used for the middle candidate.
    #[serde(default)]
    pub model_advised_rounds: Vec<u32>,
    /// Windows mouse settings, when readable.
    pub windows_mouse: Option<WindowsMouse>,
    /// Measured polling rate, if measured.
    pub polling_hz: Option<f64>,
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

/// Test length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestLength {
    /// ~15 s per candidate.
    Quick,
    /// ~25 s per candidate.
    Standard,
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
    /// Search rounds (3–5).
    pub rounds: u32,
    /// Reduce interface motion.
    pub reduce_motion: bool,
    /// Last used game id.
    pub last_game: Option<String>,
    /// Last used DPI.
    pub last_dpi: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ai_enabled: true,
            haptics_enabled: false,
            analog_keyboard_enabled: true,
            test_length: TestLength::Standard,
            rounds: 4,
            reduce_motion: false,
            last_game: None,
            last_dpi: None,
        }
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
            method: RecommendationMethod::QuadraticFit,
            observations: vec![],
            explanation: "x".into(),
            explanation_source: ExplanationSource::BuiltIn,
            model_advised_rounds: vec![],
            windows_mouse: Some(WindowsMouse { pointer_speed: 10, enhance_pointer_precision: false }),
            polling_hz: Some(1000.0),
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
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str("{\"rounds\": 3}").unwrap();
        assert_eq!(s.rounds, 3);
        assert!(s.ai_enabled);
    }
}
