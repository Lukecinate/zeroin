# How ZeroIn finds a sensitivity

This describes version 0.2. The 0.1 method (rounds of three candidates with
iterative narrowing) is kept in `zeroin-core/src/search.rs` only so 0.1
history can still be read.

## Overview

1. **Setup** collects the game, DPI and current sensitivity. Every tested
   setting is a change `x` from the current one, measured as the natural log
   of the ratio of degrees per mouse count (`x = 0` is the current sens,
   `x = ln 1.6` is 60 % faster).
2. **Blocks.** A test is a budget of blocks: 6 in Quick mode, 9 in Standard.
   Each block runs the same three drills at one setting. The player is not
   told which setting a block uses.
3. **Scoring** turns each block into score v2 (0–100).
4. **Search** picks the setting for the next block from everything measured
   so far.
5. **Decision.** After the last block the search either says *keep your
   current sens* or recommends a new one, and shows the range of settings that
   performed about equally well.

## One block

The drills rotate a virtual camera exactly as the game would at the block's
sensitivity (`degrees_per_count = yaw × sens`), so the hand motion for each
target matches the game.

| Step | Trials | Targets | Scored |
|---|---|---|---|
| Warm-up | 2 flicks | as flicks | no: absorbs the re-adjustment after a sensitivity change |
| Flick | 6 | distances stratified log-uniformly over the preset range, one per bin; 3 left, 3 right, up to ±8° vertical | yes |
| Micro-adjust | 5 | one per distance bin over the preset's micro range | yes |
| Tracking | 5 s | smooth strafing path, 2° radius, speed scaled by the preset | yes |

**Paired targets.** Every block in a test uses the same set of distances and
directions, shuffled with a new seed per block, so differences between
settings are not down to target luck.

**Presets** set the target geometry by game:

| Preset | Flick range, radius | Micro range, radius | Tracking speed | Games |
|---|---|---|---|---|
| Tactical | 8–40°, 0.9° | 1.5–5°, 0.45° | ×0.6 | Valorant, Counter-Strike 2, Rainbow Six Siege |
| Balanced | 12–45°, 1.6° | 2.5–7°, 0.7° | ×1.0 | Overwatch 2, Call of Duty, Marvel Rivals, PUBG, custom |
| Tracking-heavy | 20–90°, 2.0° | 3–8°, 1.0° | ×1.6 | Apex Legends, Fortnite, The Finals |

Input comes from Windows Raw Input on a dedicated thread, and each report
carries a timestamp from the same monotonic clock as the frame loop. All
timing comes from those timestamps:

- **Tracking** error is integrated over input time. The camera only moves on
  input reports, so between two reports the error is evaluated against the
  analytic target path in steps of at most 1 ms. The tracking score is the
  same at 30 fps and at 500 fps for the same hand motion (unit-tested).
- **Anticipation.** A click less than 120 ms after a target appears is not a
  reaction; it counts as a miss.
- **First stop.** The primary movement ends at the first pause of 35 ms or
  more; its signed error along the movement is recorded.

## Score v2

Pointing, over the 11 scored flick and micro trials:

```text
rel_i = first_stop_error_i / A_i
We_i  = max(4.133 · sd(rel) · A_i, 0.5 · W_i)       W = 2 × target radius
IDe_i = log2(A_i / We_i + 1)
MTe_i = click time (s) if hit, else 1.5 s
TPe   = mean(IDe_i / MTe_i)                          effective throughput, bits/s
P     = min(1, TPe / 8) · (0.5 + 0.5 · hit rate)
```

Tracking: `T = 0.7 · on-target share + 0.3 · clip(1 − RMS error / 4r)`.

Block score: **v2 = 100 · (0.6 · P + 0.4 · T)**.

The 0.1 components (accuracy, speed, control, tracking) are still computed
and stored for comparison but do not drive the search.

**Aim bias.** People stop short of targets by about 11 % on average whatever
their sensitivity. The stored bias is corrected for that baseline
(`bias − (−0.11)`) before any advice uses it. Advice about flying past or
stopping short is given only when the corrected bias exceeds 0.08 in the same
direction in at least two thirds of blocks. The bias never moves the search
(0.1 nudged the search with the uncorrected value, which pushed results
faster).

## Block validity

A block's data is not used, and the block runs again, when:

- the app stuttered: one frame over 250 ms, or three over 100 ms, while a
  drill was running;
- mouse input stopped for over 250 ms in the middle of a fast movement
  (0.2.2 rule): before the gap the view was 10–90 % of the way to the
  target and had not yet stopped, the trial's peak hand speed was at least
  60 °/s and the speed just before the gap was above 25 % of it; and within
  60 ms after the gap the hand was that fast again. Hand speed is measured
  over 8 ms of arrival time, so reports delivered in a burst do not inflate
  it. A reaction pause before a flick is not a gap;
- more than two clicks were anticipations, or more than three targets timed
  out;
- polling was chaotic (0.2.1, Raw Input only): the robust spread of report
  intervals, measured only while each report carries at least 2 counts, is
  more than 3× the value measured before the test (with a floor of 0.3), or
  above 0.6 when no pre-test value exists. The spread is
  1.4826 × MAD / median of the intervals, and needs at least 200 intervals.
  Slow movement, which naturally leaves polls empty, is excluded.

Pausing (Esc, Alt+Tab, minimise) also restarts the current block, without
counting against the test. Two re-runs are allowed; a third unusable block
ends the test as **Test incomplete**, with the reason, and nothing is saved.
A result always needs at least 6 valid blocks.

## Search

The search keeps a posterior over three unknowns on a grid:

- θ, the best setting (141 points from ×0.35 to ×2.8);
- k, how sharply the score falls away from θ (1, 2.5, 5, 10, 20, 40 or 80
  points per log unit squared);
- β, a learning or fatigue drift per block (−1.5 to +1.5 points).

The model is `v2 = h − k · (x − θ)² + β · block + noise`, with noise
σ = 3.5 points; the height h is removed by centring the residuals.

**Prior.** θ starts from N(0, 0.6²) around the current sens, combined with a
per-game population prior: the median pro cm/360 for the game and its spread,
from public aggregate statistics (see `zeroin-core/src/priors.rs` and
`THIRD_PARTY_NOTICES.md`). PUBG (too little public data) and custom games use
a generic prior. The
table is built offline and never fetched.

**Block positions.**

1. Blocks 1–3 bracket the current setting: ×1, ×1.6, ×0.625.
2. Later blocks go to the posterior mean ± (0.8 · sd + 0.08), alternating
   sides, and never more than ×1.25 away from the previous block.
3. Settings are rounded to the game's precision and clamped to its limits;
   the search records the setting actually tested.

**Decision.** The predicted gain of moving is `E[k] · E[θ]²` points.

| Mode | Change only if the predicted gain is at least |
|---|---|
| Conservative | 2 points |
| Balanced (default) | 1 point |
| Explorer | 0.5 points |

Below the threshold the result is **Keep your current sens**. Otherwise the
recommendation is `exp(E[θ])` × the current setting, limited to ×0.5–×2 per
session; larger moves need a second session.

**About equally good.** The expected loss of a setting x is
`E[k] · (x − x*)² + const`, so the settings within 2 points of the best form
the band `x* ± sqrt(2 / E[k])`, limited to the tested range and always
including the recommendation. The Result screen shows this range instead of a
confidence percentage: simulated interval coverage was not calibrated, so no
probability is claimed.

## Simulation

The design was chosen in a trial-level simulation of synthetic players (with
learning, fatigue and per-switch re-adaptation), comparing 0.1 against the
0.2 search at 300 players per condition. With the current sens drawn 0.4,
0.8 or 1.2 log units (sd) from the player's optimum:

| | 0.1 | 0.2 |
|---|---|---|
| Players already in their 2-point plateau told to change by more than 15 % | 72–87 % | 11–32 % |
| Players outside the plateau whose result lands inside it | 50–94 % | 65–100 % |
| 90th-percentile points lost at the result | 1.3–3.5 | 0.6–2.8 |

A block-level port of that simulation runs in CI
(`zeroin-core/tests/sim_regression.rs`, 30 synthetic players × 20 starting
points, Standard and Quick). It fails the build if, at the Balanced setting,
more than 20 % of in-plateau players are told to change (offset sd 0.4),
fewer than 70 % of out-of-plateau players are fixed (offset sd 0.8), or the
median points lost exceeds 0.5. The same input gives the same output: the
search has no randomness of its own.

## Resume

After every block the test (settings, target seed, every valid block and the
re-run count) is written atomically to `%APPDATA%\ZeroIn\unfinished-test.json`.
If the app closes mid-test, Setup offers to resume for 24 hours; the
interrupted block restarts from its first drill. Starting a new test, finishing
or ending as incomplete deletes the file.

## Input guards

- **Number entry.** Decimals accept a point or a comma (0.35 or 0,35). DPI is
  a whole number, so 1.600, 1,600 and 1 600 are read as 1600, and the screen
  says so. Text with both separators is rejected with an explanation.
- **Pixel skip.** When one mouse count turns the view by more than one pixel
  at the screen centre, Setup and Result say so and suggest a higher DPI with a
  proportionally lower in-game sensitivity (same cm/360).

## Check mouse (0.2.1)

Settings → Check mouse captures five seconds of motion reports. On Windows a
dedicated thread at time-critical priority owns the mouse Raw Input
registration for the duration, waits with `MsgWaitForMultipleObjects`
(`QS_RAWINPUT`) and drains reports with `GetRawInputBuffer`. Each read is
timestamped with the performance counter (`QueryPerformanceCounter`, through
Rust's `Instant`), and every report remembers how many reports came in the
same read. When the check ends the mouse registration is handed back to the
app's raw-input thread.

Analysis (`zeroin_core::polling::check`), for the device that moved most:

- Reports are split at pauses over 50 ms and grouped into 100 ms windows
  inside each movement. A window is *fast* when it moved at least half as many
  counts as the 90th-percentile window; at least 10 fast windows (1 s) are
  needed, and at least 300 reports.
- **Effective rate**: reports per second pooled over the fast windows. This
  counts reports, so it stays valid when Windows delivers them in batches.
- **Intervals** (inside fast windows): an interval under a quarter of the
  expected one is *bunched*. Bunched or batched reports carry the read time,
  not their own, so intervals touching them are left out of the 1st/99th
  percentiles, the standard deviation, the MAD, the median-interval rate and
  the histogram (30 bins of a tenth of the expected interval, plus overflow).
- **Skipped polls**: intervals of 1.6× the expected one or more. Reported only
  when the MAD is under 15 % of the expected interval; with more jitter skips
  cannot be told apart from it and the figure reads "not measurable".
- **Stability**: coefficient of variation of the fast-window rates.
- **Motion limit**: the share of reports carrying only 1 count. Above 35 %, the
  sensor often had nothing new to report, so the measured rate is a lower
  bound and a higher setting is reported as "not confirmed" instead of failed.
- **Rate drop on slower movement**: medium-speed windows at under 70 % of the
  fast rate while still carrying 2 or more counts per report point to a
  rate-switching or power-saving feature in the mouse.

Verdict: the worst level among the findings (Good, OK, Problem). With a rate
set in the mouse software, 88 % or more of it is Good. Lower rates are
attributed in this order: movement too slow (motion limit), the PC (over 30 %
bunched and mostly batched), a clean lower standard rate (mouse, receiver,
port or setting; listed together because Windows cannot separate them), or an
uneven rate (mouse mode or USB controller, with a rear-port retest to tell
them apart). "High-rate mode not delivered" is shown when 4000 Hz or more is
set, under half arrives, and neither slow movement nor the PC explains it.
Without a set rate, the nearest standard rate within 12 % is reported.

Device identity: the Raw Input device name (`RIDI_DEVICENAME`) is parsed for
the USB or Bluetooth vendor and product ID, and the HID product and
manufacturer strings are read with `HidD_GetProductString` /
`HidD_GetManufacturerString` on a handle opened with no access rights. A
small original vendor table names common gaming brands and marks chip makers
whose IDs appear under many brands. A wireless receiver reports its own name,
which is shown as such.

Limits: the timestamps are arrival times at ZeroIn, not the moment the mouse
sent a report, and device-side USB or radio timing cannot be seen from
Windows. Off Windows the check reports "not measurable"; developers can set
`ZEROIN_DEV_POLL=<hz>[:<jitter>]` for a labelled synthetic stream.

## Gear (0.2.1)

Mousepad surface, speed class, an optional name and mouse feet are optional
Setup steps. They are saved in settings (prefilled next time) and with each
result, and History groups results by mousepad (count, median and range of
recommended cm/360) with a filter. They are not inputs to the search, the
scores or the decision.

When a pad or feet value is present the result screen adds a short note with
sources ("Why we say this"): public CS2 pro data (CSDB.gg, n = 816) shows no
reliable difference in chosen sensitivity between control, balanced and speed
cloth pads, glass has n = 5 and feet n = 0. The note suggests, optionally,
trying about 5–10 % more cm/360 and retesting if stopping feels hard on a
faster surface, and shows those values for the measured result. A data-driven
bias table exists in code (`gear::BIAS_CELLS`): a cell can drive a bias only
with at least 50 players per group and a 95 % interval that excludes zero,
capped at ±10 %. No shipped cell qualifies, so no bias is applied.

## The local AI's role

The model never decides anything and no longer proposes test settings. After
the result is computed it may write a short explanation from the final
numbers. The text is stripped of thinking blocks, markup and links and capped
in length, and it is discarded in favour of the built-in summary if it
mentions confidence or probability, or contains any decimal number that is not
one of the result's own figures. The prompt disables Qwen3's thinking mode
(`/no_think`).
