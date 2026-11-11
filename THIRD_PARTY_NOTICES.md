# Third-party notices

ZeroIn is MIT licensed. It includes or links the following third-party components. Full licence texts are in the `licenses/` folder (installed next to `zeroin.exe`).

## Bundled in the installer

| Component | Use | Licence | Source |
|---|---|---|---|
| Qwen3-1.7B, Q4_K_M GGUF | Local AI model | Apache-2.0 (`licenses/Qwen3-LICENSE.txt`) | Model by the Qwen team, Alibaba Cloud: https://huggingface.co/Qwen/Qwen3-1.7B · GGUF quantisation by Unsloth: https://huggingface.co/unsloth/Qwen3-1.7B-GGUF (revision `d7f544eead698dbd1f15126ef60b45a1e1933222`, SHA-256 `b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897`) |

The model weights are unmodified. Apache-2.0 permits redistribution with the licence text included.

## Embedded in the executable

| Component | Use | Licence |
|---|---|---|
| Plus Jakarta Sans (Tokotype) | Interface font | SIL Open Font License 1.1 (`licenses/OFL-PlusJakartaSans.txt`) |
| Lucide icons (lucide-static 1.71.0) | Interface icons (recoloured) | ISC (`licenses/Lucide-LICENSE.txt`) |
| llama.cpp / ggml (via `llama-cpp-2` and `llama-cpp-sys-2`) | Local model runtime | MIT |
| egui, eframe, egui_extras, epaint, emath, ecolor | User interface | MIT OR Apache-2.0 |
| wgpu, naga | Rendering | MIT OR Apache-2.0 |
| winit | Windowing | Apache-2.0 |
| windows, windows-sys (Microsoft) | Win32 bindings | MIT OR Apache-2.0 |
| resvg, usvg, tiny-skia | SVG icon rendering | Apache-2.0 OR MIT / BSD-3-Clause |
| serde, serde_json | Settings and history files | MIT OR Apache-2.0 |
| libloading | Optional SDK loading | ISC |

## Data sources

The per-game starting prior in `crates/zeroin-core/src/priors.rs` (a median
cm/360 and spread per game) was derived offline from public aggregate
statistics, read on 11 November 2026. Only summary figures were used: no
per-player data, no crawling, and no text or charts were copied. Nothing is
fetched at runtime.

| Source | Figures used |
|---|---|
| CSDB.gg, "What Sensitivity Do CS2 Pros Use?" (data as of 9 Nov 2026), https://csdb.gg/data/pro-sensitivity/ | CS2 median eDPI and quartiles. Data provided by CSDB.gg, free to use with attribution. |
| SensLab, "Pro Settings Statistics" (data updated 21 Oct 2026), https://senslab.pro/pro-settings-statistics | Median cm/360 and eDPI quartiles for Valorant, CS2, Apex Legends, Overwatch 2, Fortnite, Rainbow Six Siege, Marvel Rivals, The Finals and Call of Duty |
| EsportSettings, "Average Pro Settings by Game", https://esportsettings.com/blog/average-pro-settings-by-game/ | Median and 10th–90th percentile cm/360 for CS2, Valorant, Fortnite and Apex Legends |
| Boudaoud, Spjut, Kim et al., "Mouse Sensitivity Effects in First-Person Targeting Tasks", CHI 2022 (arXiv:2203.12050) | Width check for the generic prior |

### Gear note (0.2.1)

The short mousepad and feet note on the result screen (`crates/zeroin-core/src/gear.rs`)
is original text. Its bias table holds only aggregate figures (group sizes,
median shifts and confidence intervals) derived offline; no per-player data,
pad tables, photos or review text are included, and no numeric bias is applied
because no cell meets the threshold (at least 50 players per group and a 95 %
interval that excludes zero).

| Source | Figures used |
|---|---|
| CSDB.gg mousepad pages and "Most Used Mousepads in CS2 (Pro Usage 2026)", data as of 9 Nov 2026, accessed 11 Nov 2026, https://csdb.gg/gear/rankings/mousepad/ | CS2 pad-category group sizes and median cm/360 comparisons. Data provided by CSDB.gg, free to use with attribution. |
| CSDB.gg, "What Sensitivity Do CS2 Pros Use? eDPI Distribution by Role", data as of 9 Nov 2026, accessed 11 Nov 2026, https://csdb.gg/data/pro-sensitivity/ | CS2 median cm/360 (52 cm) cited in the note |
| Richard, C. (2000), ASME IMECE; Stanford thesis "On the Identification and Haptic Display of Friction" | Friction and targeting finding, paraphrased |
| Boudaoud, Spjut, Kim et al., CHI 2022 / IEEE Transactions on Graphics 2023 | Equal-performance band (about 20–80 cm/360), paraphrased |
| Corsair, "Glass Mouse Pad vs Cloth" (updated 26 Mar 2026) | Manufacturer description of glass vs cloth, paraphrased |

### USB vendor table (0.2.1)

The vendor table in `crates/zeroin-core/src/usbid.rs` is an original, curated
list of about twenty USB vendor IDs common on gaming mice (vendor IDs are
assigned by the USB Implementers Forum and reported by the devices
themselves). It is not derived from the `usb.ids` database. Device names shown
in Check mouse are the strings the connected device reports about itself.

EloShapes is only linked: Check mouse can open https://www.eloshapes.com/mouse/browse
in the browser. No EloShapes data is fetched, stored or bundled.

These sources are not affiliated with ZeroIn.

The full dependency tree and its licences can be listed with `cargo tree` and
[`cargo about`](https://github.com/EmbarkStudios/cargo-about) or
[`cargo deny`](https://github.com/EmbarkStudios/cargo-deny) (`cargo deny list`).

## Optional, not bundled

| Component | Use | Licence |
|---|---|---|
| Wooting Analog SDK (`wooting_analog_wrapper.dll`) | Analog key depth, loaded only if the user installed it | MPL-2.0 |

## Microsoft Visual C++ runtime

Windows builds ship the Visual C++ runtime DLLs (`vcruntime140*.dll`, `msvcp140*.dll`) next to `zeroin.exe`. They are redistributable files from the Microsoft Visual Studio redistributable package, included under the Visual Studio license terms for application-local deployment. They are not covered by the MIT license.
