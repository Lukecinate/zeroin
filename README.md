<p align="center"><img src="assets/brand/png/zeroin-128.png" width="96" alt="ZeroIn logo"></p>

<h1 align="center">ZeroIn</h1>

<p align="center">Find the mouse sensitivity that fits your aim, with short drills and an optional AI model that runs entirely on your PC.<br>Windows 10 and 11.</p>

---

ZeroIn asks which game you play and how your mouse is set up, then runs blind rounds of aim drills at different sensitivities. A deterministic search narrows the range round by round until it lands on the setting you aim best with. The result comes with its cm/360, eDPI, an explanation, and the same feel converted to other games.

ZeroIn is an independent project. Its approach (testing candidate sensitivities and narrowing the range) is a general, publicly known calibration method; all code and assets here are original and MIT licensed.

## Features

- **Guided setup in plain steps.** Game, DPI, current sensitivity, Windows "Enhance pointer precision", Windows pointer speed. ZeroIn reads the last two from Windows, shows when your answer differs, and explains why acceleration on or a pointer speed other than 10 matters (and when it does not).
- **Ten games plus custom.** Valorant, Counter-Strike 2, Apex Legends, Call of Duty (MW / Warzone), Overwatch 2, Fortnite, Rainbow Six Siege, The Finals, Marvel Rivals, PUBG, or any game by its yaw value. Constants and sources: [docs/games.md](docs/games.md).
- **Three drills per candidate.** Flick, micro-adjust and tracking, rendered in the game's own rotation scale, so the hand motion matches the game.
- **Blind, deterministic search.** Each round tests three candidates (A, B, C in random order). Scoring combines accuracy, Fitts'-law speed, overshoot/correction control and tracking. See [docs/algorithm.md](docs/algorithm.md).
- **Local AI, optional.** A small bundled model (Qwen3 1.7B, 4-bit) can suggest the next middle candidate, which the algorithm clamps to its own range, and writes a plain-language explanation of the result. If the model is missing, turned off, or the CPU is unsupported, ZeroIn works the same with its built-in summary.
- **Built for fast hardware.** Raw Input on a dedicated thread with per-report timestamps; measured polling rate and stability (125 Hz to 8000 Hz); refresh-rate detection with a warning when Windows runs the display below its maximum; drills render uncapped with vsync off.
- **Hall-effect keyboards.** A raw-input keyboard check shows the fastest release-to-press time per key (rapid trigger), and analog key depth when the Wooting Analog SDK is installed.
- **Results and conversions.** Recommended sensitivity, cm/360, in/360, eDPI and the equivalent in every other supported game. History is saved locally.
- **Apply to Windows, with undo.** One action sets "Enhance pointer precision" off and pointer speed to 10 for your Windows account, after showing current and new values. Your previous values are saved first and Restore previous puts them back. In-game sensitivity is never written; you enter the recommended value in the game.
- **Simple to drive.** Three sections (Find my sens, History, Settings), one primary button per screen, Enter for next, Esc for back, number keys for options, F1–F3 for sections.

## Install

Download `ZeroIn-Setup-<version>-x64.exe` from [Releases](https://github.com/Lukecinate/zeroin/releases) and run it. The installer includes the AI model (about 1.1 GB) and installs per user by default (no admin rights needed).

A portable zip without the model is also published. To add the model to the portable build, place a `.gguf` file in a `models` folder next to `zeroin.exe`, or in `%APPDATA%\ZeroIn\models`.

Verify downloads with `SHA256SUMS.txt` from the same release:

```powershell
Get-FileHash .\ZeroIn-Setup-0.1.1-x64.exe -Algorithm SHA256
```

### Requirements

- Windows 10 or 11, 64-bit.
- For the local AI: a CPU with AVX2 and FMA (most Intel CPUs since 2013 and AMD since 2015) and about 2 GB of free memory. Without them ZeroIn runs without the AI.

## Privacy

ZeroIn is fully local. It has no accounts, no telemetry, no analytics and makes no network requests. Settings, history and the pointer-settings backup are stored in `%APPDATA%\ZeroIn` as JSON. Windows settings are changed only when you confirm Apply to Windows or Restore previous. The AI model runs on your CPU and only sees aggregate drill scores.

## Build from source

```powershell
# App without the AI (no C/C++ toolchain needed)
cargo build --release -p zeroin-app

# App with the AI (needs CMake, Clang/LLVM and the MSVC build tools)
$env:GGML_AVX2 = "ON"; $env:GGML_FMA = "ON"; $env:GGML_F16C = "ON"
cargo build --release -p zeroin-app --features llm
```

The platform-independent logic (game database, math, drills, scoring, search, AI-output validation) lives in `crates/zeroin-core` and tests on any OS:

```sh
cargo test -p zeroin-core
```

Releases are built by GitHub Actions on `windows-latest` from a version tag ([release.yml](.github/workflows/release.yml)). The model is downloaded from a pinned Hugging Face revision and checked against its SHA-256 before it is packaged; it is not stored in this repository.

## Project layout

| Path | Contents |
|---|---|
| `crates/zeroin-core` | Games, math, drills, scoring, search, advisor validation, polling stats, springs, history format |
| `crates/zeroin-app` | egui/eframe app, Win32 integration, local model runner, optional hardware |
| `assets/` | Logo (SVG, PNG, ICO), Lucide icons, Plus Jakarta Sans |
| `installer/` | Inno Setup script |
| `docs/` | Game constants, algorithm, design notes |

## Status

Version 0.1.1. See [CHANGELOG.md](CHANGELOG.md) for what is included.

### Known issues

- Haptic feedback is not implemented: no haptic mouse offers an open SDK or documented protocol yet.
- Display refresh is read for the primary display only.
- Constants for The Finals, Marvel Rivals and PUBG are community estimates; conversions involving them are approximate.
- The local AI needs a CPU with AVX2/FMA; without it ZeroIn uses its built-in summary.
- Installer and app are not code-signed yet, so Windows SmartScreen may warn on first run.

## License

MIT, see [LICENSE](LICENSE). Third-party components and their licences are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
