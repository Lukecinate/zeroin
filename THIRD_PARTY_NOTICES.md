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

The full dependency tree and its licences can be listed with `cargo tree` and
[`cargo about`](https://github.com/EmbarkStudios/cargo-about) or
[`cargo deny`](https://github.com/EmbarkStudios/cargo-deny) (`cargo deny list`).

## Optional, not bundled

| Component | Use | Licence |
|---|---|---|
| Wooting Analog SDK (`wooting_analog_wrapper.dll`) | Analog key depth, loaded only if the user installed it | MPL-2.0 |

## Microsoft Visual C++ runtime

Windows builds ship the Visual C++ runtime DLLs (`vcruntime140*.dll`, `msvcp140*.dll`) next to `zeroin.exe`. They are redistributable files from the Microsoft Visual Studio redistributable package, included under the Visual Studio license terms for application-local deployment. They are not covered by the MIT license.
