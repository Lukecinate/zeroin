//! Sensitivity arithmetic: cm/360, in/360, eDPI and cross-game conversion.

use crate::games::GameProfile;

/// Centimetres per inch.
pub const CM_PER_INCH: f64 = 2.54;

/// Physical mouse travel (cm) for one full 360° turn.
pub fn cm_per_360(dpi: f64, deg_per_count: f64) -> f64 {
    inches_per_360(dpi, deg_per_count) * CM_PER_INCH
}

/// Physical mouse travel (inches) for one full 360° turn.
pub fn inches_per_360(dpi: f64, deg_per_count: f64) -> f64 {
    360.0 / (dpi * deg_per_count)
}

/// Degrees per count needed to reach `cm360` at `dpi`.
pub fn deg_per_count_for_cm360(dpi: f64, cm360: f64) -> f64 {
    360.0 * CM_PER_INCH / (dpi * cm360)
}

/// eDPI as commonly quoted: `dpi * sens`. Only comparable within one game.
pub fn edpi(dpi: f64, sens: f64) -> f64 {
    dpi * sens
}

/// Convert a sensitivity between games (and optionally DPIs) keeping cm/360 constant.
pub fn convert(sens: f64, from: &GameProfile, from_dpi: f64, to: &GameProfile, to_dpi: f64) -> f64 {
    let dpc_from = from.model.deg_per_count(sens);
    let dpc_to = dpc_from * from_dpi / to_dpi;
    to.model.sens_for_deg_per_count(dpc_to)
}

/// Windows pointer-speed slider (1–20) to cursor multiplier with
/// "Enhance pointer precision" off. Notch 10 is 1:1.
pub fn windows_pointer_multiplier(speed: u32) -> f64 {
    const TABLE: [f64; 20] =
        [0.03125, 0.0625, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5];
    TABLE[(speed.clamp(1, 20) - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::{by_id, GameProfile};

    fn g(id: &str) -> GameProfile {
        GameProfile::from_game(by_id(id).unwrap())
    }

    #[test]
    fn cs2_reference_value() {
        // 800 DPI, CS2 sens 2.0 -> ~25.98 cm/360 (widely published reference).
        let cm = cm_per_360(800.0, g("cs2").model.deg_per_count(2.0));
        assert!((cm - 25.977).abs() < 0.01, "{cm}");
    }

    #[test]
    fn cs2_to_valorant() {
        let v = convert(2.0, &g("cs2"), 800.0, &g("valorant"), 800.0);
        assert!((v - 0.6286).abs() < 1e-3, "{v}");
    }

    #[test]
    fn dpi_change_halves_sens() {
        let v = convert(0.5, &g("valorant"), 800.0, &g("valorant"), 1600.0);
        assert!((v - 0.25).abs() < 1e-12);
    }

    #[test]
    fn conversion_preserves_cm360_for_all_games() {
        let base = g("cs2");
        let cm = cm_per_360(800.0, base.model.deg_per_count(1.5));
        for game in crate::games::GAMES {
            let to = GameProfile::from_game(game);
            let s = convert(1.5, &base, 800.0, &to, 1600.0);
            let cm2 = cm_per_360(1600.0, to.model.deg_per_count(s));
            assert!((cm - cm2).abs() < 1e-9, "{}", game.id);
        }
    }

    #[test]
    fn inverse_helpers() {
        let dpc = deg_per_count_for_cm360(800.0, 30.0);
        assert!((cm_per_360(800.0, dpc) - 30.0).abs() < 1e-12);
        assert!((inches_per_360(800.0, dpc) * CM_PER_INCH - 30.0).abs() < 1e-12);
        assert_eq!(edpi(800.0, 0.4), 320.0);
    }

    #[test]
    fn pointer_table() {
        assert_eq!(windows_pointer_multiplier(10), 1.0);
        assert_eq!(windows_pointer_multiplier(0), 0.03125);
        assert_eq!(windows_pointer_multiplier(99), 3.5);
    }
}
