# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.2.1] - 2026-11-11

### Added
- **Check mouse** (Settings → Devices → Check mouse). A five-second capture of mouse Raw Input, read in buffered batches (`GetRawInputBuffer`) on a time-critical thread and timestamped with the performance counter. Reports the effective rate, the rate from the median interval, 1st/99th-percentile intervals, jitter (standard deviation and MAD), skipped polls, bunched and batched delivery, the share of 1-count reports, rate stability over time, a rate graph and an interval histogram. Every figure is computed from the capture; figures that cannot be measured say so with the reason.
- **Polling verdict.** Good, OK or Problem against the rate set in the mouse software (chips, keys 1–7), with plain-language findings and the likely source: the mouse or its settings, the USB port or wireless link, the PC, or how the mouse was moved. A high setting that is far from delivered and not explained by slow movement or the PC is flagged as "High-rate mode not delivered". Movement too slow to fill every poll is reported as "not confirmed", not as a mouse fault.
- **Device identity.** The device's own product and manufacturer strings (HID) and its USB or Bluetooth vendor/product ID from the Raw Input device path, with a small original vendor table for common gaming brands and chip makers. Wireless receivers are shown as receivers. A "View specs on EloShapes" button opens eloshapes.com in the browser; no EloShapes data is fetched or bundled.
- **Polling-jitter validity rule.** With Raw Input, a block whose report-interval spread (robust CV while the hand moves fast) is more than 3× the pre-test value, with a floor, or above an absolute limit without a pre-test value, runs again like other invalid blocks.
- **Optional gear.** Two skippable Setup steps: mousepad surface (Cloth, Hybrid, Glass, Hard/plastic, Other), speed class (Speed, Balanced, Control) and an optional name; mouse feet (Stock, PTFE, Glass, Ceramic, Other). Values are prefilled next time, stored with each result, shown in Result and History, and History can filter and compare results by mousepad. Free text is cleaned (control and invisible formatting characters removed, at most 60 characters).
- **Gear note.** When gear is filled in, the result screen shows a short, cited note: public CS2 pro data shows no reliable sensitivity difference by pad category, so ZeroIn does not change the number; if stopping feels hard on a faster surface, trying about 5–10 % more cm/360 and retesting is offered as an option, with the values for the measured result. The note never changes a measured number. A data-driven bias table is in place but disabled: a bias needs at least 50 players per group and a 95 % interval excluding zero, capped at ±10 %, and no current cell qualifies.
- **Gear lookup in your browser** (opt-in, off by default). Opens a web search for the typed mousepad name in the default browser. The query is sanitised; the app itself makes no network connection.

### Changed
- Setup has 8 steps; the two gear steps can be skipped with Enter.
- About and README privacy text describe the two user-initiated browser links.

### Known limitations
- Check mouse was verified with synthetic timestamp streams and a development mock only; it has not yet been run against real 4000–8000 Hz hardware.
- Timestamps are arrival times at ZeroIn; device-side USB or wireless timing is not visible from Windows.
- Gear lookup opens a browser search only; there is no automatic material analysis of product pages.

## [0.2.0] - 2026-11-11

### Changed
- **New search.** A test is now a budget of blocks at one sensitivity each instead of rounds of three candidates. Blocks 1–3 bracket the current setting (×1, ×1.6, ×0.625); later blocks are placed from a grid posterior over the best setting, the score curvature and a per-block drift, never more than ×1.25 from the previous block. The prior combines the current sensitivity with a per-game population prior built offline from public aggregate pro-settings statistics.
- **Keep unless it clearly helps.** A change is recommended only when the predicted gain reaches the threshold of the chosen mode: Conservative (2 points), Balanced (1 point, default) or Explorer (0.5 points). Otherwise the result is "Keep your current sens". A single session moves at most ×0.5–×2.
- **Drills and scoring.** Each block runs 2 unscored warm-up flicks, 6 flicks stratified over the distance range, 5 micro-adjusts and 5 s of tracking. Every block in a test uses the same targets in a new order. Target sizes follow the game type (tactical, balanced, tracking-heavy). The search objective is the new score v2: effective Fitts throughput with hit rate, plus tracking measured over raw-input time instead of frame time.
- **Result screen.** Shows the range of settings within about 2 points of the best, with a chart of the tested settings, the current value and the recommendation. The confidence percentage is removed: simulated interval coverage did not support a probability. cm/360 is shown to the nearest half centimetre.
- **Local AI.** The model only writes the explanation; it no longer proposes test settings. An explanation that mentions confidence or contains a decimal number that is not one of the result's figures is discarded in favour of the built-in summary.
- Test length: Quick is 6 blocks (about 2.5 min), Standard 9 blocks (about 4 min). The Rounds setting is replaced by the mode setting.

### Added
- Session budget: a segmented progress bar and the time left between blocks, and session progress and time left during drills.
- Resume: progress is saved after every block to `%APPDATA%\ZeroIn\unfinished-test.json`, and Setup offers to resume an interrupted test for 24 hours.
- Block validity: a block is run again when the app stutters (a frame over 250 ms, or three over 100 ms), mouse input stops for over 250 ms mid-movement, more than two clicks come under 120 ms after a target appears, or more than three targets time out. Up to two re-runs; a third unusable block ends with "Test incomplete" and nothing is saved. A result needs at least 6 valid blocks.
- Clicks under 120 ms after a target appears count as misses (anticipations).
- Number entry accepts a decimal point or comma; DPI typed as 1.600, 1,600 or 1 600 is read as 1600 and the screen says so.
- Pixel-skip warning on Setup and Result when one mouse count turns the view by more than one pixel at the screen centre, with a DPI and sensitivity pair that keeps the same cm/360.
- CI runs a simulation regression check of the search (30 synthetic players × 20 starting points, Quick and Standard).
- Data-source attribution for the per-game priors in THIRD_PARTY_NOTICES.md (CSDB.gg, SensLab, EsportSettings, Boudaoud et al. 2022).

### Fixed
- Aim-bias advice is corrected for the typical 11 % undershoot, and the bias no longer moves the search. In 0.1 the uncorrected bias nudged results towards faster settings.
- The drill crosshair is drawn in whole physical pixels, so it stays sharp at 125 % and 150 % display scaling.

### Known limitations
- The search was tuned in simulation with synthetic players; no study with real players has been run yet.
- The polling-jitter validity rule is not implemented.
- What one session learns is not carried into the next session's prior.
- The pixel-skip check uses the primary display's width and the game's default field of view.

## [0.1.1] - 2026-11-10

### Added
- **Apply to Windows** on the Result screen, in Settings, and next to the Setup warnings when acceleration is on or pointer speed is not 10. It sets "Enhance pointer precision" off and pointer speed to 10 (6 of 11) for the current Windows user, after a confirmation that lists current and new values. No admin rights are needed.
- **Restore previous**: the values from before the change are saved to `%APPDATA%\ZeroIn\mouse-backup.json` first and can be put back from the same places. Both actions read the settings back and report whether Windows accepted them.
- The Result screen says where the recommended value goes: ZeroIn does not change in-game settings; enter the value in the game.

### Fixed
- The mouse cursor stayed visible during drills. It is now hidden and confined to the window from the start to the end of each drill, and released immediately on test end, Esc/pause, focus loss (Alt+Tab), minimise, app exit and on a crash. It is hidden again when you continue a paused drill. Drill input still comes from Raw Input, so measurements are unchanged.
- History showed times in UTC; it now uses the PC's local time zone, including daylight saving time.

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
