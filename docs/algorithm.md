# How ZeroIn finds a sensitivity

## Overview

1. **Setup** collects the game, DPI and current sensitivity. Every candidate is
   expressed as a multiplier `m` of the current sensitivity.
2. **Rounds.** Each round tests three candidates (low, mid, high) in a random,
   blind order labelled A, B and C. Each candidate runs three drills.
3. **Scoring** turns each candidate's drills into a 0–100 score.
4. **Search** narrows the range using the scores.
5. **Result** fits the final observations and rounds to the game's precision.

## Drills

The drills rotate a virtual camera exactly as the game would at the candidate
sensitivity (`degrees_per_count = yaw × sens`), so the physical hand motion for
each target matches the game.

| Drill | Targets | Measures |
|---|---|---|
| Flick | 12–45° away, 1.6° radius | hit, time to target, click time, overshoot, first-stop error, corrections |
| Micro-adjust | 2.5–7° away, 0.7° radius | same as flick |
| Tracking | strafing target, 2° radius | share of time on target, mean angular error |

Input comes from Windows Raw Input on a dedicated thread. Each report carries
a timestamp from the same monotonic clock as the frame loop, so stop detection
and timing do not depend on frame rate. Drill frames are drawn uncapped with
vsync off and without any interface animation.

## Score

| Component | Weight | Definition |
|---|---|---|
| Accuracy | 0.35 | hits / trials over flick and micro-adjust |
| Speed | 0.25 | median Fitts'-law throughput of hits, `log2(D/W + 1) / time`, normalised to 8 bits/s |
| Control | 0.20 | `1 − 2 × mean(overshoot / distance) − 0.15 × mean(corrections)` over hits |
| Tracking | 0.20 | `0.7 × on-target share + 0.3 × (1 − mean error / 4r)` |

A signed **bias** (mean first-stop error / distance) records whether the player
tends to overshoot (positive) or stop short (negative).

## Search

The search works in log space (`x = ln m`) so 0.8× and 1.25× are equally far
from 1×. It starts at `x = 0` with a half-span of `ln 1.6` and global limits of
0.35× to 2.8×.

After each round:

1. The best-scoring candidate becomes the new centre (ties go to the middle).
2. The centre is nudged by `−0.3 × half-span × bias`, so overshooting pulls the
   next round lower and stopping short pulls it higher.
3. The half-span halves when the middle candidate won and shrinks only to 80 %
   when an edge won, so the search can walk outward if the start was far off.

After the last round, a least-squares quadratic is fitted to the scores of the
last two rounds. If it is concave and its peak lies inside the tested range,
the peak is the result; otherwise the best single observation is used.
Confidence combines the score gap between the best and the rest with the
number of rounds completed.

The search is deterministic: the same scores always produce the same result.
Unit tests drive it with synthetic players whose optimum ranges from 0.55× to
1.9× and require the result to land within 8 % of the optimum.

## The local AI's role

The model never decides the result.

- **Next candidate.** Between rounds the model sees the score table and may
  propose a multiplier for the next middle candidate. The reply must be JSON;
  it is parsed strictly, non-finite or non-positive values are rejected, and
  the value is clamped to the inner half of the round's range. Low and high
  candidates are always the algorithm's own.
- **Explanation.** After the result is computed, the model is given the final
  numbers and asked for a short explanation without suggesting a different
  value. The text is stripped of thinking blocks, markup and links and capped
  in length. If no usable text arrives, the built-in summary is shown.

Both prompts disable Qwen3's thinking mode (`/no_think`) to keep replies short.
