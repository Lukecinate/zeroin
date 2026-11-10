# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-10

First public release.

### Added
- Guided setup: game, mouse DPI, current sensitivity, Windows "Enhance pointer precision" and pointer speed, with automatic reading of the Windows values, mismatch notices and explanations.
- Game database for Valorant, Counter-Strike 2, Apex Legends, Call of Duty (MW / Warzone), Overwatch 2, Fortnite, Rainbow Six Siege, The Finals, Marvel Rivals and PUBG, plus custom games by yaw value. Estimated constants are labelled.
- Flick, micro-adjust and tracking drills rendered in the selected game's rotation scale, uncapped with vsync off.
- Blind three-candidate rounds with a deterministic iterative-narrowing search and quadratic refinement.
- Scoring from accuracy, Fitts'-law throughput, overshoot/correction control and tracking.
- Optional local AI (Qwen3 1.7B, Q4_K_M, via llama.cpp on the CPU) that suggests the next middle candidate within clamped bounds and writes the result explanation. Built-in explanation when the model is unavailable.
- Result screen with recommended sensitivity, cm/360, in/360, eDPI, confidence and conversions to every other supported game.
- Local history and settings in `%APPDATA%\ZeroIn`.
- Raw Input on a dedicated thread; polling-rate and stability measurement; display refresh detection and warning.
- Keyboard check for Hall-effect / rapid-trigger keyboards; analog key depth via the Wooting Analog SDK when installed.
- Spring-based interface motion (screen transitions, selection indicators, toggles, sliders, result entrance), with a Reduce motion setting.
- Inno Setup installer bundling the model, portable zip without the model, SHA-256 checksums.

### Known limitations
- Haptic feedback is not implemented: no haptic mouse currently offers an open SDK or documented protocol. The setting is shown as unavailable.
- Display refresh is read for the primary display only.
- Constants for The Finals, Marvel Rivals and PUBG are community estimates; conversions involving them are approximate.
- The local AI needs a CPU with AVX2/FMA.
