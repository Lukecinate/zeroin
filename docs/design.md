# Interface notes

**Direction:** a calm, precise instrument. One decision per screen; the number is the hero.

- **Palette.** Dark neutrals (`#111214` background, `#191A1D` surfaces, `#2B2D32` borders), off-white text, one coral accent `#FF5C47` taken from the logo. Coral is reserved for the primary action, the active navigation marker, drill targets, progress bars and the result number with its marker. Amber marks warnings and estimates.
- **No decoration without a reason.** No gradients, glass, glow or decorative shadows. Corner radius 8 px for controls and 12 px for cards.
- **Type.** Plus Jakarta Sans (regular, semibold, bold).
- **Icons.** Lucide, one family, 1.75 px stroke, tinted per state.
- **Navigation.** Side rail with three sections. In Find my sens, a three-step bar (Setup → Test → Result). One coral button per screen whose label names the next step. Enter = next, Esc = back one step, number keys pick options, F1–F3 switch sections.
- **Motion.** Every animated value is a spring (`zeroin_core::spring`), critically damped for movement and slightly underdamped (ζ ≈ 0.78) for snapping indicators, settling in about 220 ms. Motion responds to new input mid-flight and the app repaints at the display's refresh rate only while something moves. The result number enters from the "Best so far" label as a shared element. Drills never animate. Settings → Reduce motion turns interface animation off.
- **Logo.** Bold lowercase "z." in near-black on a coral rounded tile. Vector source in `assets/brand/zeroin-logo.svg`; PNGs at 16–1024 px and a multi-size `.ico` are generated from it.
