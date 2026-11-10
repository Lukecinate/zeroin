//! Game database: how each supported title turns mouse counts into rotation.
//!
//! Values and their sources are documented in `docs/games.md`. Entries whose
//! published constants disagree are marked [`Confidence::Estimate`] and the UI
//! says so next to every conversion that uses them.

use serde::{Deserialize, Serialize};

/// How a game's sensitivity setting maps to degrees of rotation per mouse count.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SensModel {
    /// `degrees_per_count = yaw * sens`.
    Linear {
        /// Degrees per count at sensitivity 1.0.
        yaw: f64,
    },
    /// `degrees_per_count = base * 10^(sens / divisor)` (non-linear sliders such as PUBG).
    Exponential {
        /// Degrees per count when the slider is at 0.
        base: f64,
        /// Slider units per decade.
        divisor: f64,
    },
}

impl SensModel {
    /// Degrees of rotation produced by one mouse count at `sens`.
    pub fn deg_per_count(&self, sens: f64) -> f64 {
        match *self {
            SensModel::Linear { yaw } => yaw * sens,
            SensModel::Exponential { base, divisor } => base * 10f64.powf(sens / divisor),
        }
    }

    /// Inverse of [`Self::deg_per_count`]: the slider value that yields `dpc`.
    pub fn sens_for_deg_per_count(&self, dpc: f64) -> f64 {
        match *self {
            SensModel::Linear { yaw } => dpc / yaw,
            SensModel::Exponential { base, divisor } => divisor * (dpc / base).log10(),
        }
    }

    /// True when the model is a finite, positive, usable mapping.
    pub fn is_valid(&self) -> bool {
        match *self {
            SensModel::Linear { yaw } => yaw.is_finite() && yaw > 0.0,
            SensModel::Exponential { base, divisor } => base.is_finite() && base > 0.0 && divisor.is_finite() && divisor > 0.0,
        }
    }
}

/// How well a game's constant is corroborated by independent sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Engine-documented or corroborated by multiple independent sources.
    Verified,
    /// Sources disagree or the slider is non-linear; treat as a starting point.
    Estimate,
}

/// A static entry in the built-in game list.
#[derive(Clone, Copy, Debug)]
pub struct Game {
    /// Stable identifier stored in history files.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Sensitivity model.
    pub model: SensModel,
    /// Value used only to prefill the "current sensitivity" field.
    pub prefill_sens: f64,
    /// Decimal places the in-game field accepts.
    pub decimals: u8,
    /// Smallest value the in-game slider accepts.
    pub sens_min: f64,
    /// Largest value the in-game slider accepts.
    pub sens_max: f64,
    /// Horizontal field of view (16:9, degrees) used to render drills for this game.
    pub hfov_deg: f64,
    /// Confidence in the constant.
    pub confidence: Confidence,
    /// Unit or caveat shown next to the field, if any.
    pub note: &'static str,
}

/// Built-in games, in the order shown in Setup.
pub const GAMES: &[Game] = &[
    Game {
        id: "valorant",
        name: "Valorant",
        model: SensModel::Linear { yaw: 0.07 },
        prefill_sens: 0.4,
        decimals: 3,
        sens_min: 0.01,
        sens_max: 10.0,
        hfov_deg: 103.0,
        confidence: Confidence::Verified,
        note: "",
    },
    Game {
        id: "cs2",
        name: "Counter-Strike 2",
        model: SensModel::Linear { yaw: 0.022 },
        prefill_sens: 1.25,
        decimals: 3,
        sens_min: 0.01,
        sens_max: 10.0,
        hfov_deg: 106.26,
        confidence: Confidence::Verified,
        note: "Assumes default m_yaw 0.022",
    },
    Game {
        id: "apex",
        name: "Apex Legends",
        model: SensModel::Linear { yaw: 0.022 },
        prefill_sens: 2.0,
        decimals: 2,
        sens_min: 0.2,
        sens_max: 20.0,
        hfov_deg: 106.26,
        confidence: Confidence::Verified,
        note: "",
    },
    Game {
        id: "cod",
        name: "Call of Duty (MW / Warzone)",
        model: SensModel::Linear { yaw: 0.0066 },
        prefill_sens: 5.0,
        decimals: 2,
        sens_min: 0.01,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Verified,
        note: "Legacy/relative sensitivity type",
    },
    Game {
        id: "overwatch2",
        name: "Overwatch 2",
        model: SensModel::Linear { yaw: 0.0066 },
        prefill_sens: 15.0,
        decimals: 2,
        sens_min: 0.01,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Verified,
        note: "",
    },
    Game {
        id: "fortnite",
        name: "Fortnite",
        model: SensModel::Linear { yaw: 0.005555 },
        prefill_sens: 7.0,
        decimals: 1,
        sens_min: 0.1,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Verified,
        note: "Percent slider: enter 7 for 7%",
    },
    Game {
        id: "r6s",
        name: "Rainbow Six Siege",
        model: SensModel::Linear { yaw: 0.005_729_577_95 },
        prefill_sens: 10.0,
        decimals: 0,
        sens_min: 1.0,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Verified,
        note: "Assumes MouseSensitivityMultiplierUnit 0.02",
    },
    Game {
        id: "the_finals",
        name: "The Finals",
        model: SensModel::Linear { yaw: 0.001 },
        prefill_sens: 40.0,
        decimals: 1,
        sens_min: 1.0,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Estimate,
        note: "Sources disagree (0.001 vs 0.0066)",
    },
    Game {
        id: "marvel_rivals",
        name: "Marvel Rivals",
        model: SensModel::Linear { yaw: 0.0175 },
        prefill_sens: 2.0,
        decimals: 2,
        sens_min: 0.01,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Estimate,
        note: "Sources disagree (0.0175 / 0.022 / 0.0066)",
    },
    Game {
        id: "pubg",
        name: "PUBG: Battlegrounds",
        model: SensModel::Exponential { base: 0.004_444, divisor: 50.0 },
        prefill_sens: 50.0,
        decimals: 0,
        sens_min: 1.0,
        sens_max: 100.0,
        hfov_deg: 103.0,
        confidence: Confidence::Estimate,
        note: "Non-linear slider; approximate",
    },
];

/// Identifier used for user-defined games.
pub const CUSTOM_ID: &str = "custom";

/// Look up a built-in game by id.
pub fn by_id(id: &str) -> Option<&'static Game> {
    GAMES.iter().find(|g| g.id == id)
}

/// An owned, serialisable game description (built-in or custom).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameProfile {
    /// Built-in id or [`CUSTOM_ID`].
    pub id: String,
    /// Display name.
    pub name: String,
    /// Sensitivity model.
    pub model: SensModel,
    /// Decimal places for display/rounding.
    pub decimals: u8,
    /// Horizontal FOV used for drills.
    pub hfov_deg: f64,
    /// Confidence in the constant.
    pub confidence: Confidence,
}

impl GameProfile {
    /// Build from a built-in entry.
    pub fn from_game(g: &Game) -> Self {
        GameProfile {
            id: g.id.to_string(),
            name: g.name.to_string(),
            model: g.model,
            decimals: g.decimals,
            hfov_deg: g.hfov_deg,
            confidence: g.confidence,
        }
    }

    /// A user-defined linear game. Returns `None` for a non-positive or non-finite yaw.
    pub fn custom(name: &str, yaw: f64) -> Option<Self> {
        let model = SensModel::Linear { yaw };
        if !model.is_valid() {
            return None;
        }
        let name = name.trim();
        Some(GameProfile {
            id: CUSTOM_ID.to_string(),
            name: if name.is_empty() { "Custom game".into() } else { name.chars().take(48).collect() },
            model,
            decimals: 4,
            hfov_deg: 103.0,
            confidence: Confidence::Estimate,
        })
    }

    /// Round a sensitivity to the precision the game's field accepts.
    pub fn round_sens(&self, sens: f64) -> f64 {
        let p = 10f64.powi(self.decimals as i32);
        (sens * p).round() / p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_models_valid() {
        for (i, a) in GAMES.iter().enumerate() {
            assert!(a.model.is_valid(), "{}", a.id);
            assert!(a.sens_min < a.sens_max);
            assert!(a.prefill_sens >= a.sens_min && a.prefill_sens <= a.sens_max, "{}", a.id);
            for b in &GAMES[i + 1..] {
                assert_ne!(a.id, b.id);
            }
        }
    }

    #[test]
    fn models_round_trip() {
        for g in GAMES {
            let s = g.prefill_sens;
            let back = g.model.sens_for_deg_per_count(g.model.deg_per_count(s));
            assert!((back - s).abs() < 1e-9, "{}: {s} -> {back}", g.id);
        }
    }

    #[test]
    fn custom_rejects_bad_yaw() {
        assert!(GameProfile::custom("x", 0.0).is_none());
        assert!(GameProfile::custom("x", f64::NAN).is_none());
        assert!(GameProfile::custom("x", -1.0).is_none());
        assert_eq!(GameProfile::custom("  ", 0.02).unwrap().name, "Custom game");
    }

    #[test]
    fn rounding_uses_decimals() {
        let v = GameProfile::from_game(by_id("valorant").unwrap());
        assert_eq!(v.round_sens(0.123456), 0.123);
        let r = GameProfile::from_game(by_id("r6s").unwrap());
        assert_eq!(r.round_sens(12.6), 13.0);
    }
}
