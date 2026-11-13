<p align="center"><img src="assets/brand/png/zeroin-128.png" width="96" alt="ZeroIn logo"></p>

<h1 align="center">ZeroIn</h1>

<p align="center">Find the mouse sensitivity that fits your aim, with short drills and an optional AI model that runs entirely on your PC.<br>Windows 10 and 11.</p>

---

ZeroIn asks which game you play and how your mouse is set up, then runs a short budget of aim-drill blocks at sensitivities chosen by a deterministic search, without telling you which is which. The result either says to keep your current sensitivity or recommends a new one, and shows the range of settings that performed about equally well, with cm/360, eDPI, an explanation and the same feel converted to other games.

ZeroIn is an independent project. Its approach (testing candidate sensitivities and narrowing the range) is a general, publicly known calibration method; all code and assets here are original and MIT licensed.

## Screenshots

| Setup | Test |
|---|---|
| ![Setup: a step asks whether Enhance pointer precision is on and compares the answer with what Windows reports](docs/screenshots/setup.png) | ![Test: a flick drill with the block number, time left and session progress](docs/screenshots/test.png) |
| **Result** | **History** |
| ![Result: the new sensitivity with cm/360 and eDPI, the range of about-equally-good settings on a chart, and a summary](docs/screenshots/result.png) | ![History: past results stored on this PC only](docs/screenshots/history.png) |
| **Settings** | **Check mouse** |
| ![Settings: test length, when to suggest a change, local AI and devices](docs/screenshots/settings.png) | ![Check mouse: the rate set in the mouse software, the device, a verdict with the effective rate and findings](docs/screenshots/check-mouse.png) |

Test, Result and Settings were captured from version 0.2.0, Setup and History from 0.1.1, during scripted demo runs: the scores and the summary reflect synthetic input rather than a real player. Check mouse was captured from version 0.2.1 with the development mock stream (`ZEROIN_DEV_POLL`, a synthetic uneven 2500 Hz stream against an 8000 Hz setting), as the screen itself states; it is not a measurement of a real device.

## Features

- **Guided setup in plain steps.** Game, DPI, current sensitivity, Windows "Enhance pointer precision", Windows pointer speed, then optional mousepad and mouse feet. ZeroIn reads the two Windows settings, shows when your answer differs, and explains why acceleration on or a pointer speed other than 10 matters (and when it does not).
- **Gear notes, never gear maths.** Mousepad surface, speed class and name, and mouse feet are optional and skippable. They are stored with each result, shown in History and Result, and History can filter and compare results by mousepad. They never change the measured sensitivity or any score: public data shows no reliable sensitivity difference by pad type, so the result screen only adds a short, cited note when gear is filled in.
- **Ten games plus custom.** Valorant, Counter-Strike 2, Apex Legends, Call of Duty (MW / Warzone), Overwatch 2, Fortnite, Rainbow Six Siege, The Finals, Marvel Rivals, PUBG, or any game by its yaw value. Constants and sources: [docs/games.md](docs/games.md).
- **Short, fair blocks.** Each block runs two warm-up flicks, six flicks, five micro-adjusts and five seconds of tracking, rendered in the game's own rotation scale. Every block uses the same targets in a new order, with sizes set per game type (tactical, balanced, tracking-heavy). Timing comes from raw input timestamps, so frame rate does not change the score.
- **A search that keeps what already works.** Three bracket blocks (current, ×1.6, ×0.625), then adaptive blocks placed from a Bayesian model of your scores, started from a per-game prior built from public pro-settings statistics. A change is recommended only when the predicted gain clears a threshold you choose: Conservative, Balanced (default) or Explorer. See [docs/algorithm.md](docs/algorithm.md).
- **Quick or Standard, with a budget.** Quick is 6 blocks (about 2.5 minutes), Standard 9 (about 4). A progress bar and the time left are shown between blocks and during drills.
- **Clear results.** "Keep your current sens" when nothing scored clearly better, otherwise the new value, plus the range of settings within about 2 points of the best. No confidence percentage is shown, because simulation did not support one. Fewer than 6 usable blocks gives "Test incomplete", never a guess.
- **Resume.** Progress is saved after every block; an interrupted test can be resumed from Setup within 24 hours.
- **Input guards.** Blocks hit by stutter, dropped mouse input, chaotic polling, anticipated clicks or repeated timeouts run again (up to twice). Numbers can be typed with a point or a comma, and DPI written as 1.600 or 1,600 is read as 1600. A warning appears when one mouse count turns the view by more than a pixel.
- **Local AI, optional.** A small bundled model (Qwen3 1.7B, 4-bit) can write a plain-language explanation of the computed result. It never chooses test settings or changes a number, and text that states other numbers is discarded. If the model is missing, turned off, or the CPU is unsupported, ZeroIn works the same with its built-in summary.
- **Check mouse.** Settings → Check mouse measures the polling rate your mouse really delivers. Move the mouse fast for five seconds; ZeroIn reads Raw Input in buffered batches on a high-priority thread, timestamps it with the performance counter and reports the effective rate, the rate from the median interval, 1st/99th-percentile intervals, jitter (standard deviation and MAD), skipped polls, bunched delivery, stability over time, a rate graph and an interval histogram. Pick the rate set in your mouse software and the verdict (Good, OK or Problem) says whether it is delivered, flags a high-rate mode that does not hold up, and names the likely source: the mouse, the USB port or wireless link, the PC, or the way the mouse was moved. The device's own name, manufacturer string and USB vendor/product ID are shown; a wireless receiver is shown as the receiver. Anything that cannot be measured says so and why.
- **Fits any screen.** Pages sit in a centred column that scrolls (wheel, scrollbar, Page Up / Page Down / Home / End) and wraps text instead of cutting it off. The interface follows the Windows display scale and, on Auto, the window size, from a 14" laptop to a 55" TV; Settings → Interface scale also offers 90–150 %. The scale never changes test results.
- **Built for fast hardware.** Raw Input on a dedicated thread with per-report timestamps; refresh-rate detection with a warning when Windows runs the display below its maximum; drills render uncapped with vsync off.
- **Hall-effect keyboards.** A raw-input keyboard check shows the fastest release-to-press time per key (rapid trigger), and analog key depth when the Wooting Analog SDK is installed.
- **Results and conversions.** Recommended sensitivity, cm/360, in/360, eDPI and the equivalent in every other supported game. History is saved locally, and 0.1 history still opens.
- **Apply to Windows, with undo.** One action sets "Enhance pointer precision" off and pointer speed to 10 for your Windows account, after showing current and new values. Your previous values are saved first and Restore previous puts them back. In-game sensitivity is never written; you enter the recommended value in the game.
- **Neutral settings for a test only.** Before a test, ZeroIn can ask to set precision off and speed 10 just while drills run (Ask / Always / Never in Settings). Your values are journalled first and restored when a drill stops, on Esc, focus loss, close, after a crash at the next launch, or at sign-out. The change is session-only and never saved to your Windows profile; a small notice shows on the drill screen while it is active. Declining is fine: drills read raw mouse counts, which these settings do not scale.
- **Simple to drive.** Three sections (Find my sens, History, Settings), one primary button per screen, Enter for next, Esc for back, number keys for options, F1–F3 for sections.

## Install

Download `ZeroIn-Setup-<version>-x64.exe` from [Releases](https://github.com/Lukecinate/zeroin/releases) and run it. The installer includes the AI model (about 1.1 GB) and installs per user by default (no admin rights needed).

A portable zip without the model is also published. To add the model to the portable build, place a `.gguf` file in a `models` folder next to `zeroin.exe`, or in `%APPDATA%\ZeroIn\models`.

Verify downloads with `SHA256SUMS.txt` from the same release:

```powershell
Get-FileHash .\ZeroIn-Setup-0.2.3-x64.exe -Algorithm SHA256
```

### Requirements

- Windows 10 or 11, 64-bit.
- For the local AI: a CPU with AVX2 and FMA (most Intel CPUs since 2013 and AMD since 2015) and about 2 GB of free memory. Without them ZeroIn runs without the AI.

## Privacy

ZeroIn is fully local. It has no accounts, no telemetry, no analytics and makes no network requests. Settings, history (including any mousepad and feet you enter), an unfinished test and the pointer-settings backup are stored in `%APPDATA%\ZeroIn` as JSON. The per-game priors and the USB vendor table are constant tables built into the app; nothing is downloaded. Windows settings are changed only when you confirm Apply to Windows or Restore previous, or agree to neutral settings for a test (restored afterwards; the previous values are kept in `mouse-temp.json` until then). The AI model runs on your CPU and only sees aggregate drill scores.

Two buttons open a page in your own web browser, and only when you click them:

- **Gear lookup** (off by default; turn it on in Settings). Setup then offers "Search specs in your browser", which opens a DuckDuckGo search for the mousepad name you typed. The name is cleaned first (control characters removed, at most 60 characters, letters, digits and basic punctuation only). Your browser sends that text to the search site; ZeroIn sends nothing, reads nothing back and never sends test data, hardware IDs or PC details. You then pick surface and speed yourself.
- **View specs on EloShapes** in Check mouse opens eloshapes.com/mouse/browse. ZeroIn does not fetch, store or bundle any EloShapes data.

Check mouse reads the device name and HID strings locally with read-only queries; nothing about the device leaves the PC.

## Build from source

```powershell
# App without the AI (no C/C++ toolchain needed)
cargo build --release -p zeroin-app

# App with the AI (needs CMake, Clang/LLVM and the MSVC build tools)
$env:GGML_AVX2 = "ON"; $env:GGML_FMA = "ON"; $env:GGML_F16C = "ON"
cargo build --release -p zeroin-app --features llm
```

The platform-independent logic (game database, math, drills, scoring, search, priors, AI-output validation) lives in `crates/zeroin-core` and tests on any OS:

```sh
cargo test -p zeroin-core
```

This includes a simulation regression check of the search (`crates/zeroin-core/tests/sim_regression.rs`) that CI runs on every push.

Releases are built by GitHub Actions on `windows-latest` from a version tag ([release.yml](.github/workflows/release.yml)). The model is downloaded from a pinned Hugging Face revision and checked against its SHA-256 before it is packaged; it is not stored in this repository.

## Project layout

| Path | Contents |
|---|---|
| `crates/zeroin-core` | Games, math, drills, scoring, Bayesian search and per-game priors, number parsing, advisor validation, polling analysis, USB IDs, gear notes, springs, history and resume format |
| `crates/zeroin-app` | egui/eframe app, Win32 integration, local model runner, optional hardware |
| `assets/` | Logo (SVG, PNG, ICO), Lucide icons, Plus Jakarta Sans |
| `installer/` | Inno Setup script |
| `docs/` | Game constants, algorithm, design notes |

## Status

Version 0.2.3. See [CHANGELOG.md](CHANGELOG.md) for what is included.

### Known issues

- Haptic feedback is not implemented: no haptic mouse offers an open SDK or documented protocol yet.
- Display refresh is read for the primary display only.
- Constants for The Finals, Marvel Rivals and PUBG are community estimates; conversions involving them are approximate.
- The local AI needs a CPU with AVX2/FMA; without it ZeroIn uses its built-in summary.
- Installer and app are not code-signed yet, so Windows SmartScreen may warn on first run.
- The search and its settings were tuned in simulation with synthetic players; a study with real players has not been run yet.
- Check mouse times reports when Windows hands them to ZeroIn, not when the mouse sends them; device-side USB or wireless timing is not visible from Windows. It has been verified with synthetic streams only, not yet on real 4000–8000 Hz hardware.
- Gear lookup opens a browser search only; there is no automatic material analysis of product pages.
- Neutral settings for a test were verified with mocked Windows settings; the Win32 calls are compile-checked but have not yet been run on a Windows PC.
- The search does not yet carry what it learned into your next session; each test starts from the per-game prior.
- The pixel-skip warning uses the primary display's width and the game's default field of view.

## License

MIT, see [LICENSE](LICENSE). Third-party components and their licences are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
