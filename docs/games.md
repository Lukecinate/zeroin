# Game constants

ZeroIn converts an in-game sensitivity into degrees of rotation per mouse count
("yaw"). For most games this is linear:

```
degrees_per_count = yaw × sensitivity
cm/360            = 360 / (DPI × degrees_per_count) × 2.54
```

Developers rarely publish these numbers, so the values below come from public
sensitivity references that cross-check each other. Entries where published
values disagree are marked **Estimate** in the app; conversions that involve
them are approximate. The values were reviewed in November 2026.

| Game | Model | Yaw (° per count at sens 1) | Status | Notes |
|---|---|---|---|---|
| Valorant | linear | 0.07 | Verified | |
| Counter-Strike 2 | linear | 0.022 | Verified | Source engine default `m_yaw 0.022` |
| Apex Legends | linear | 0.022 | Verified | Source-engine heritage, same scale as CS2 |
| Call of Duty (MW / Warzone) | linear | 0.0066 | Verified | "Legacy"/relative sensitivity type |
| Overwatch 2 | linear | 0.0066 | Verified | |
| Fortnite | linear | 0.005555 | Verified | Per 1 % of the X-axis slider (enter 7 for 7 %) |
| Rainbow Six Siege | linear | 0.00572957795 | Verified | Assumes the default `MouseSensitivityMultiplierUnit` of 0.02 |
| The Finals | linear | 0.001 | Estimate | Sources disagree: 0.001 (consistent with its 1–100 slider) and 0.0066 |
| Marvel Rivals | linear | 0.0175 | Estimate | Sources disagree: 0.0175, 0.022 and 0.0066 are all in circulation |
| PUBG: Battlegrounds | exponential | 0.004444 × 10^(sens / 50) | Estimate | Non-linear slider; approximate |
| Other / custom | linear | user-entered | Estimate | |

## Sources

- DCPROSENS, "Complete FPS Yaw Value Reference Table (Sourced)": https://dcprosens.com/blog/complete-fps-yaw-value-reference/
- WinroGames, "Mouse Sensitivity Yaw Values for 23 FPS Games": https://winrogames.com/tools/sensitivity-yaw-values
- SensLab sensitivity converter (yaw list): https://senslab.pro/sensitivity
- AimPro, "Sensitivity Converter for 19 FPS Games" (yaw table, cross-checked November 2026): https://aimprofps.com/en/sensitivity-converter/
- SensSwap game list (including PUBG's exponential model): https://sensswap.com/games/
- Mansone sensitivity converter (yaw table): https://mansone.com/tools/sensitivity-converter

## Reference checks

These are covered by unit tests in `crates/zeroin-core/src/math.rs`:

- CS2 sensitivity 2.0 at 800 DPI is 25.98 cm/360.
- CS2 2.0 converts to Valorant 0.629 at the same DPI.
- Converting any game to any other preserves cm/360 exactly.

## Drill field of view

The horizontal field of view used to draw drills (103° for most games, 106.26°
for CS2 and Apex at 16:9) only affects how targets look. It does not change
the rotation per count, so it has no effect on the recommended sensitivity.

## Updating a value

Games change their sensitivity scales in patches. To propose a change, open an
issue or pull request with at least two independent sources. Values live in
`crates/zeroin-core/src/games.rs`.
