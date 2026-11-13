//! Mouse polling-rate estimation from raw-input timestamps.

use serde::{Deserialize, Serialize};

/// Standard USB polling rates.
pub const STANDARD_RATES: [u32; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

/// Result of analysing a burst of motion reports.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PollingStats {
    /// Reports per second while the mouse moves fast enough to fill every poll.
    pub rate_hz: f64,
    /// Closest standard rate, if within 30 %.
    pub nominal_hz: Option<u32>,
    /// Share of fast-movement windows whose report count is within ±8 % of the rate (0–1).
    pub stability: f64,
    /// Number of reports analysed.
    pub samples: usize,
}

impl PollingStats {
    /// A rate is considered stable when at least 80 % of fast windows are close to it.
    pub fn is_stable(&self) -> bool {
        self.stability >= 0.8
    }
}

/// Minimum reports in fast windows needed for a reading.
pub const MIN_SAMPLES: usize = 200;
/// Window length for the live meter.
const METER_WINDOW_US: u64 = 100_000;
/// Fast windows needed for a reading (half a second of fast movement).
const METER_MIN_WINDOWS: usize = 5;

/// Analyse timestamps (µs, ascending) of consecutive motion reports from one device.
///
/// A mouse only sends a report when it has moved, so slow movement leaves
/// polls empty and the plain mean interval reads low (a 1000 Hz mouse moved
/// slowly can average 700 Hz). The arrival times are also bunched when
/// Windows delivers several reports at once. So reports are counted in
/// 100 ms windows (gaps over 20 ms split them), and the rate is the pooled
/// count of the windows with at least 80 % of the 90th-percentile count,
/// that is, while the hand moved fast enough to fill every poll.
pub fn analyze(timestamps_us: &[u64]) -> Option<PollingStats> {
    let mut counts: Vec<u32> = Vec::new();
    let mut it = timestamps_us.iter().copied();
    let first = it.next()?;
    let (mut prev, mut win_start, mut n) = (first, first, 0u32);
    for t in it {
        if t.saturating_sub(prev) > 20_000 {
            // Pause: drop the partial window and start again here.
            win_start = t;
            n = 0;
        } else {
            while t > win_start + METER_WINDOW_US {
                counts.push(n);
                n = 0;
                win_start += METER_WINDOW_US;
            }
            n += 1;
        }
        prev = t;
    }
    if counts.len() < METER_MIN_WINDOWS {
        return None;
    }
    let mut sorted = counts.clone();
    sorted.sort_unstable();
    let p90 = sorted[((sorted.len() - 1) as f64 * 0.9).round() as usize];
    let fast: Vec<u32> = counts.into_iter().filter(|&c| c > 0 && c as f64 >= 0.8 * p90 as f64).collect();
    let total: u32 = fast.iter().sum();
    if fast.len() < METER_MIN_WINDOWS || (total as usize) < MIN_SAMPLES {
        return None;
    }
    let per_window = total as f64 / fast.len() as f64;
    let rate = per_window * 1e6 / METER_WINDOW_US as f64;
    let close = fast.iter().filter(|&&c| (c as f64 - per_window).abs() <= 0.08 * per_window).count();
    let nominal = STANDARD_RATES
        .iter()
        .copied()
        .min_by(|a, b| ((*a as f64 - rate).abs()).total_cmp(&(*b as f64 - rate).abs()))
        .filter(|n| (*n as f64 - rate).abs() / (*n as f64) <= 0.3);
    Some(PollingStats { rate_hz: rate, nominal_hz: nominal, stability: close as f64 / fast.len() as f64, samples: total as usize })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Timestamps for `secs` seconds at `hz`, optionally with arrival bursts of `burst` reports.
    fn stream(start: u64, hz: f64, secs: f64, burst: u64) -> Vec<u64> {
        let n = (hz * secs) as u64;
        let dt = 1e6 / hz;
        (0..n).map(|i| start + (((i / burst) * burst) as f64 * dt) as u64).collect()
    }

    #[test]
    fn detects_1000hz() {
        let ts: Vec<u64> = (0..3000).map(|i| i * 1000 + (i % 3) * 50).collect();
        let s = analyze(&ts).unwrap();
        assert_eq!(s.nominal_hz, Some(1000));
        assert!(s.is_stable());
    }

    #[test]
    fn slow_movement_does_not_read_low() {
        // 1000 Hz mouse: fast stretches, and slow stretches where only every
        // second or third poll has movement. The plain mean read ~750 Hz here.
        let mut ts = Vec::new();
        let mut t = 0u64;
        for round in 0..6 {
            for _ in 0..500 {
                t += 1000;
                ts.push(t);
            }
            for k in 0..400 {
                t += if (k + round) % 3 == 0 { 3000 } else { 2000 };
                ts.push(t);
            }
        }
        let naive = 1e6 * (ts.len() - 1) as f64 / (ts[ts.len() - 1] - ts[0]) as f64;
        assert!(naive < 800.0, "scenario reproduces the low reading ({naive:.0})");
        let s = analyze(&ts).unwrap();
        assert_eq!(s.nominal_hz, Some(1000));
        assert!((s.rate_hz - 1000.0).abs() < 30.0, "{}", s.rate_hz);
    }

    #[test]
    fn bunched_arrival_and_8000hz() {
        // Windows hands over 8 reports at a time; the count per window is still right.
        let s = analyze(&stream(0, 8000.0, 1.5, 8)).unwrap();
        assert_eq!(s.nominal_hz, Some(8000));
        let s = analyze(&stream(0, 125.0, 6.0, 1)).unwrap();
        assert_eq!(s.nominal_hz, Some(125));
    }

    #[test]
    fn detects_8000hz_and_ignores_pauses() {
        let mut ts = stream(0, 8000.0, 0.4, 1);
        let last = *ts.last().unwrap();
        ts.extend(stream(last + 500_000, 8000.0, 0.4, 1));
        let s = analyze(&ts).unwrap();
        assert_eq!(s.nominal_hz, Some(8000));
    }

    #[test]
    fn unstable_is_flagged_and_short_is_none() {
        // Report rate wandering between 1000 and 820 Hz from window to window.
        let mut ts = Vec::new();
        let mut t = 0u64;
        for w in 0..30 {
            let dt = if w % 2 == 0 { 1220 } else { 1000 };
            for _ in 0..(100_000 / dt) {
                t += dt;
                ts.push(t);
            }
        }
        assert!(!analyze(&ts).unwrap().is_stable());
        assert!(analyze(&[0, 1000, 2000]).is_none());
    }
}

// ---------------------------------------------------------------------------
// Full polling check (0.2.1)

/// One motion report captured during a polling check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    /// Arrival time in nanoseconds (high-resolution performance counter).
    pub t_ns: u64,
    /// Horizontal counts.
    pub dx: i32,
    /// Vertical counts.
    pub dy: i32,
    /// Number of reports Windows handed over in the same buffered read
    /// (1 = read on its own).
    pub batch: u32,
}

/// Overall assessment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// No problems found.
    Good,
    /// Usable, with minor findings.
    Ok,
    /// A problem worth fixing.
    Problem,
}

impl Verdict {
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Good => "Good",
            Verdict::Ok => "OK",
            Verdict::Problem => "Problem",
        }
    }
}

/// Most likely origin of a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The mouse or its settings.
    Mouse,
    /// The connection: USB port, hub, cable or wireless link.
    Connection,
    /// The PC: CPU load, power saving, USB controller, Windows delivery.
    System,
    /// More than one cause fits; a second test can separate them.
    Unclear,
    /// The way the mouse was moved during the check.
    Movement,
}

impl Source {
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Source::Mouse => "Likely source: the mouse or its settings",
            Source::Connection => "Likely source: USB port, hub or wireless link",
            Source::System => "Likely source: the PC",
            Source::Unclear => "Source: unclear, see the suggested test",
            Source::Movement => "Source: how the mouse was moved",
        }
    }
}

/// One plain-language finding.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// Severity.
    pub level: Verdict,
    /// Short headline.
    pub title: String,
    /// Explanation and what to try.
    pub detail: String,
    /// Likely origin.
    pub source: Source,
}

/// Rate in one 100 ms window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RateWindow {
    /// Window start, seconds since the first report.
    pub t_s: f64,
    /// Reports per second in the window.
    pub rate_hz: f64,
    /// Counts moved in the window.
    pub counts: f64,
    /// Counted as fast movement (used for the headline figures).
    pub fast: bool,
}

/// Full result of a polling check. Every figure is computed from the captured
/// reports; nothing is estimated from the device name or claimed rate.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckReport {
    /// Reports captured.
    pub reports: usize,
    /// Seconds of fast movement analysed.
    pub fast_seconds: f64,
    /// Reports per second during fast movement (pooled over 100 ms windows).
    pub effective_hz: f64,
    /// 1 / median interval between reports that arrived separately.
    pub interval_hz: Option<f64>,
    /// Nearest standard rate within 12 % of `effective_hz`.
    pub standard_hz: Option<u32>,
    /// 1st-percentile interval, µs (reports that arrived separately).
    pub p1_us: Option<f64>,
    /// 99th-percentile interval, µs.
    pub p99_us: Option<f64>,
    /// Standard deviation of intervals, µs.
    pub std_us: Option<f64>,
    /// Median absolute deviation of intervals, µs.
    pub mad_us: Option<f64>,
    /// Share of intervals about twice the expected length or more (skipped
    /// polls). `None` when timing is too uneven to tell skips from jitter.
    pub skipped_frac: Option<f64>,
    /// Share of reports that arrived bunched (under a quarter of the expected interval).
    pub bunched_frac: f64,
    /// Share of reports handed over in a buffered read with others.
    pub batched_frac: f64,
    /// Share of reports in fast windows carrying at most 1 count.
    pub low_count_frac: f64,
    /// Coefficient of variation of the window rates (stability over time).
    pub rate_cv: f64,
    /// Median rate in medium-speed windows divided by `effective_hz`.
    pub slow_ratio: Option<f64>,
    /// Average counts per report in medium-speed windows.
    pub slow_counts_per_report: Option<f64>,
    /// Rate over time.
    pub windows: Vec<RateWindow>,
    /// Interval histogram: bin width in µs and counts (last bin = overflow).
    pub hist_bin_us: f64,
    /// Histogram counts.
    pub hist: Vec<u32>,
    /// Overall verdict.
    pub verdict: Verdict,
    /// Findings, most important first.
    pub findings: Vec<Finding>,
    /// The claimed rate could not be confirmed from this movement.
    pub claim_unconfirmed: bool,
    /// Nominal high rate (4000 Hz or more) that is far from delivered and not
    /// explained by slow movement or the PC.
    pub gimmick_suspect: bool,
}

/// Why a check could not produce figures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotMeasurable {
    /// Fewer reports than [`CHECK_MIN_REPORTS`].
    TooFewReports(usize),
    /// Less than a second of fast movement.
    TooLittleFastMovement,
}

impl NotMeasurable {
    /// Plain-language explanation.
    pub fn message(&self) -> String {
        match self {
            NotMeasurable::TooFewReports(n) => format!(
                "Not measurable: only {n} motion reports were captured (at least {CHECK_MIN_REPORTS} are needed). Move the mouse quickly in wide circles for the whole check."
            ),
            NotMeasurable::TooLittleFastMovement => {
                "Not measurable: less than a second of fast movement was captured. Move the mouse quickly in wide circles for the whole check.".into()
            }
        }
    }
}

/// Minimum reports for a check.
pub const CHECK_MIN_REPORTS: usize = 300;
const WINDOW_NS: u64 = 100_000_000;
const PAUSE_NS: u64 = 50_000_000;
const MIN_FAST_WINDOWS: usize = 10;

fn median(v: &mut [f64]) -> Option<f64> {
    crate::math::median_in_place(v)
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    let i = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn nearest_standard(rate: f64, tol: f64) -> Option<u32> {
    STANDARD_RATES
        .iter()
        .copied()
        .min_by(|a, b| ((*a as f64 - rate).abs()).partial_cmp(&(*b as f64 - rate).abs()).unwrap_or(std::cmp::Ordering::Equal))
        .filter(|n| (*n as f64 - rate).abs() / (*n as f64) <= tol)
}

/// Analyse a polling check. `claimed_hz` is the rate set in the mouse's
/// software, when the player knows it.
pub fn check(reports: &[Report], claimed_hz: Option<u32>) -> Result<CheckReport, NotMeasurable> {
    if reports.len() < CHECK_MIN_REPORTS {
        return Err(NotMeasurable::TooFewReports(reports.len()));
    }
    let t0 = reports[0].t_ns;
    // 1. Segments without pauses, then 100 ms windows fully inside a segment.
    let mut window_of: Vec<Option<usize>> = vec![None; reports.len()];
    let mut windows: Vec<RateWindow> = Vec::new();
    let mut win_reports: Vec<Vec<usize>> = Vec::new();
    let mut seg_start = 0usize;
    for i in 1..=reports.len() {
        let end = i == reports.len() || reports[i].t_ns.saturating_sub(reports[i - 1].t_ns) > PAUSE_NS;
        if !end {
            continue;
        }
        let (s, e) = (seg_start, i); // [s, e)
        let ws = reports[s].t_ns;
        let we = reports[e - 1].t_ns;
        let mut w0 = ws;
        let mut j = s;
        while w0 + WINDOW_NS <= we {
            let mut idx = Vec::new();
            let mut counts = 0.0;
            while j < e && reports[j].t_ns < w0 + WINDOW_NS {
                if reports[j].t_ns >= w0 {
                    idx.push(j);
                    counts += (reports[j].dx.unsigned_abs() + reports[j].dy.unsigned_abs()) as f64;
                }
                j += 1;
            }
            let wi = windows.len();
            for &k in &idx {
                window_of[k] = Some(wi);
            }
            windows.push(RateWindow {
                t_s: (w0 - t0) as f64 / 1e9,
                rate_hz: idx.len() as f64 * 1e9 / WINDOW_NS as f64,
                counts,
                fast: false,
            });
            win_reports.push(idx);
            w0 += WINDOW_NS;
        }
        seg_start = i;
    }
    // 2. Fast windows: at least half the 90th-percentile movement.
    let mut counts_sorted: Vec<f64> = windows.iter().map(|w| w.counts).collect();
    counts_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if counts_sorted.is_empty() {
        return Err(NotMeasurable::TooLittleFastMovement);
    }
    let p90 = percentile(&counts_sorted, 0.9);
    for w in windows.iter_mut() {
        w.fast = p90 > 0.0 && w.counts >= 0.5 * p90;
    }
    let fast_idx: Vec<usize> = (0..windows.len()).filter(|&i| windows[i].fast).collect();
    if fast_idx.len() < MIN_FAST_WINDOWS {
        return Err(NotMeasurable::TooLittleFastMovement);
    }
    let fast_rates: Vec<f64> = fast_idx.iter().map(|&i| windows[i].rate_hz).collect();
    // Pooled over all fast windows: avoids the ±1 report quantisation of a
    // single 100 ms window at low rates.
    let mean_rate = fast_rates.iter().sum::<f64>() / fast_rates.len() as f64;
    let effective = mean_rate;
    if effective <= 0.0 {
        return Err(NotMeasurable::TooLittleFastMovement);
    }
    let rate_cv = (fast_rates.iter().map(|r| (r - mean_rate).powi(2)).sum::<f64>() / fast_rates.len() as f64).sqrt() / mean_rate.max(1e-9);
    let t_exp_us = 1e6 / effective;

    // 3. Intervals inside fast windows.
    let is_fast = |k: usize| window_of[k].is_some_and(|w| windows[w].fast);
    let (mut n_fast_reports, mut low_count, mut batched, mut bunched, mut skipped, mut n_int) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut clean: Vec<f64> = Vec::new();
    for k in 0..reports.len() {
        if !is_fast(k) {
            continue;
        }
        n_fast_reports += 1;
        let r = reports[k];
        if r.dx.unsigned_abs() + r.dy.unsigned_abs() <= 1 {
            low_count += 1;
        }
        if r.batch > 1 {
            batched += 1;
        }
        if k == 0 || window_of[k - 1] != window_of[k] {
            continue;
        }
        let d_us = (r.t_ns - reports[k - 1].t_ns) as f64 / 1000.0;
        n_int += 1;
        if d_us < 0.25 * t_exp_us {
            bunched += 1;
            continue;
        }
        if d_us >= 1.6 * t_exp_us {
            skipped += 1;
        }
        // Leave out intervals touching a bunched or batched report: their
        // arrival time is the read time, not the report time.
        let prev_bunched = k >= 2
            && window_of[k - 2] == window_of[k]
            && ((reports[k - 1].t_ns.saturating_sub(reports[k - 2].t_ns)) as f64 / 1000.0) < 0.25 * t_exp_us;
        if r.batch <= 1 && reports[k - 1].batch <= 1 && !prev_bunched {
            clean.push(d_us);
        }
    }
    let nf = n_fast_reports.max(1) as f64;
    let ni = n_int.max(1) as f64;
    clean.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let enough_clean = clean.len() >= 100;
    let (interval_hz, p1, p99, std, mad) = if enough_clean {
        let med = percentile(&clean, 0.5);
        let mean = clean.iter().sum::<f64>() / clean.len() as f64;
        let std = (clean.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / clean.len() as f64).sqrt();
        let mut dev: Vec<f64> = clean.iter().map(|d| (d - med).abs()).collect();
        let mad = median(&mut dev).unwrap_or(0.0);
        (Some(1e6 / med), Some(percentile(&clean, 0.01)), Some(percentile(&clean, 0.99)), Some(std), Some(mad))
    } else {
        (None, None, None, None, None)
    };
    // Histogram over 0..3 expected intervals in 30 bins, plus overflow.
    let hist_bin_us = t_exp_us / 10.0;
    let mut hist = vec![0u32; 31];
    for d in &clean {
        let b = ((d / hist_bin_us) as usize).min(30);
        hist[b] += 1;
    }

    // 4. Medium-speed windows: rate drop on slower movement.
    let med_idx: Vec<usize> = (0..windows.len()).filter(|&i| !windows[i].fast && windows[i].counts >= 0.15 * p90).collect();
    let (slow_ratio, slow_cpr) = if med_idx.len() >= 5 {
        let mut r: Vec<f64> = med_idx.iter().map(|&i| windows[i].rate_hz).collect();
        let m = median(&mut r).unwrap_or(0.0);
        let n: usize = med_idx.iter().map(|&i| win_reports[i].len()).sum();
        let c: f64 = med_idx.iter().map(|&i| windows[i].counts).sum();
        (Some(m / effective), (n > 0).then(|| c / n as f64))
    } else {
        (None, None)
    };

    let skipped_raw = skipped as f64 / ni;
    let mut rep = CheckReport {
        reports: reports.len(),
        fast_seconds: fast_idx.len() as f64 * WINDOW_NS as f64 / 1e9,
        effective_hz: effective,
        interval_hz,
        standard_hz: nearest_standard(effective, 0.12),
        p1_us: p1,
        p99_us: p99,
        std_us: std,
        mad_us: mad,
        skipped_frac: None,
        bunched_frac: bunched as f64 / ni,
        batched_frac: batched as f64 / nf,
        low_count_frac: low_count as f64 / nf,
        rate_cv,
        slow_ratio,
        slow_counts_per_report: slow_cpr,
        windows,
        hist_bin_us,
        hist,
        verdict: Verdict::Good,
        findings: Vec::new(),
        claim_unconfirmed: false,
        gimmick_suspect: false,
    };
    // Skips are only separable from jitter when timing is otherwise even.
    let rel_mad = rep.mad_us.map(|m| m / t_exp_us);
    rep.skipped_frac = rel_mad.filter(|m| *m < 0.15).map(|_| skipped_raw);
    assess(&mut rep, claimed_hz);
    Ok(rep)
}

fn add(rep: &mut CheckReport, level: Verdict, source: Source, title: String, detail: String) {
    rep.findings.push(Finding { level, title, detail, source });
}

/// Turn the figures into a verdict and findings.
fn assess(rep: &mut CheckReport, claimed: Option<u32>) {
    let eff = rep.effective_hz;
    let motion_limited = rep.low_count_frac > 0.35;
    let system_bunching = rep.bunched_frac > 0.3 && rep.batched_frac > 0.5 * rep.bunched_frac;
    let rel_mad = rep.mad_us.map(|m| m * eff / 1e6);
    let clean_timing = rel_mad.is_some_and(|m| m < 0.15) && rep.rate_cv < 0.1 && rep.bunched_frac < 0.1;

    if let Some(c) = claimed {
        let ratio = eff / c as f64;
        if ratio >= 0.88 {
            add(
                rep,
                Verdict::Good,
                Source::Mouse,
                format!("Delivers the {c} Hz that is set"),
                format!("Measured {eff:.0} reports per second while moving fast."),
            );
        } else if motion_limited {
            rep.claim_unconfirmed = true;
            add(
                rep,
                Verdict::Ok,
                Source::Movement,
                format!("{c} Hz not confirmed: movement too slow for this DPI"),
                format!(
                    "{:.0} % of reports carried only 1 count, so the sensor had nothing new to send on many polls. The measured {eff:.0} Hz is a lower bound. Move faster, or raise DPI for the check only, to measure {c} Hz.",
                    rep.low_count_frac * 100.0
                ),
            );
        } else if system_bunching {
            add(
                rep,
                Verdict::Problem,
                Source::System,
                format!("Only {eff:.0} Hz reached ZeroIn, delivered in bursts"),
                format!(
                    "Windows handed over {:.0} % of reports bunched together, so the PC was slow to read input. Common causes: CPU load, a power-saving plan, or a busy USB controller. Close heavy apps, use the High performance power plan and test again before judging the mouse.",
                    rep.bunched_frac * 100.0
                ),
            );
        } else if clean_timing && rep.standard_hz.is_some_and(|s| (s as f64) < 0.88 * c as f64) {
            let s = rep.standard_hz.unwrap_or(0);
            rep.gimmick_suspect = c >= 4000 && ratio < 0.5;
            add(
                rep,
                Verdict::Problem,
                Source::Unclear,
                format!("Steady at {s} Hz, not the {c} Hz that is set"),
                format!(
                    "Timing is even, so the mouse is polling cleanly at a lower rate. Check that the setting was saved, that high rates are allowed in this mode (some mice need their own high-rate receiver or a cable), and that the mouse is not on a hub or front-panel port. If all of that is ruled out, the mouse does not deliver its {c} Hz mode."
                ),
            );
        } else {
            rep.gimmick_suspect = c >= 4000 && ratio < 0.5;
            let level = if ratio < 0.75 { Verdict::Problem } else { Verdict::Ok };
            add(
                rep,
                level,
                Source::Unclear,
                format!("{eff:.0} Hz measured, {c} Hz set"),
                format!(
                    "The rate is {:.0} % of the setting and not steady. Either the mouse's {c} Hz mode does not hold up, or the USB port or controller cannot keep up. Plug it into a rear USB port on the motherboard with other high-rate devices unplugged and test again: if the result stays like this, the {c} Hz mode is not delivering.",
                    ratio * 100.0
                ),
            );
        }
    } else {
        match rep.standard_hz {
            Some(s) => add(rep, Verdict::Good, Source::Mouse, format!("Polling at about {s} Hz"), format!("Measured {eff:.0} reports per second while moving fast. Pick the rate set in the mouse software above to check it against that.")),
            None if motion_limited => {
                rep.claim_unconfirmed = true;
                add(
                    rep,
                    Verdict::Ok,
                    Source::Movement,
                    format!("About {eff:.0} Hz, limited by movement"),
                    "Most reports carried only 1 count, so the true rate may be higher. Move faster or raise DPI for the check only.".into(),
                );
            }
            None => add(
                rep,
                Verdict::Ok,
                Source::Unclear,
                format!("About {eff:.0} Hz, between standard rates"),
                "This usually means a higher rate is set but not fully delivered. Pick the rate set in the mouse software above to see a detailed verdict.".into(),
            ),
        }
    }

    // Missing polls.
    if let Some(sk) = rep.skipped_frac.filter(|s| !motion_limited && *s > 0.03) {
        let level = if sk > 0.10 { Verdict::Problem } else { Verdict::Ok };
        add(
            rep,
            level,
            Source::Connection,
            format!("{:.1} % of polls were skipped", sk * 100.0),
            "Intervals about twice the normal length mean reports went missing while the mouse was moving. On wireless, move the receiver closer (an extension cable on the desk helps) and away from Wi-Fi routers and USB 3 ports; on a cable, avoid hubs and front-panel ports.".into(),
        );
    }
    // Bursty delivery (when not already the main finding).
    if !system_bunching && rep.bunched_frac > 0.1 {
        add(
            rep,
            Verdict::Ok,
            Source::System,
            format!("{:.0} % of reports arrived bunched", rep.bunched_frac * 100.0),
            "Windows delivered some reports together instead of one by one. At 4000–8000 Hz this is common and mostly reflects how fast the PC reads input; games see the same bursts.".into(),
        );
    }
    // Uneven intervals.
    if let Some(m) = rel_mad {
        if m > 0.25 {
            let level = if m > 0.5 { Verdict::Problem } else { Verdict::Ok };
            add(
                rep,
                level,
                Source::Unclear,
                "Uneven intervals between reports".into(),
                format!(
                    "The typical deviation is {:.0} % of the expected interval. This usually comes from the USB controller, hubs or CPU power states rather than the sensor. Try a rear USB port, the High performance power plan, or a lower polling rate.",
                    m * 100.0
                ),
            );
        }
    } else {
        add(
            rep,
            Verdict::Ok,
            Source::System,
            "Interval figures not measurable".into(),
            "Too few reports arrived on their own to measure intervals. The rate figure is still valid because it counts reports.".into(),
        );
    }
    // Stability over time.
    if rep.rate_cv > 0.15 {
        let level = if rep.rate_cv > 0.3 { Verdict::Problem } else { Verdict::Ok };
        add(
            rep,
            level,
            if motion_limited { Source::Movement } else { Source::Unclear },
            "Rate changed a lot during the check".into(),
            format!("The rate varied by {:.0} % between 100 ms windows of fast movement.", rep.rate_cv * 100.0),
        );
    }
    // Rate drop on slower movement with enough counts per report.
    if let (Some(r), Some(cpr)) = (rep.slow_ratio, rep.slow_counts_per_report) {
        if r < 0.7 && cpr >= 2.0 {
            add(
                rep,
                Verdict::Ok,
                Source::Mouse,
                "Rate drops on slower movement".into(),
                format!(
                    "At medium speed the rate fell to {:.0} % although reports still carried {cpr:.1} counts each. That points to a rate-switching or power-saving feature in the mouse; check its software.",
                    r * 100.0
                ),
            );
        }
    }
    rep.verdict = rep
        .findings
        .iter()
        .map(|f| f.level)
        .max_by_key(|v| match v {
            Verdict::Good => 0,
            Verdict::Ok => 1,
            Verdict::Problem => 2,
        })
        .unwrap_or(Verdict::Good);
}

// ---------------------------------------------------------------------------
// Polling jitter during sensitivity tests

/// Timing spread of motion reports while the hand moves fast enough to fill
/// every poll.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Jitter {
    /// Median interval, µs.
    pub median_us: f64,
    /// Robust coefficient of variation: 1.4826 × MAD / median.
    pub robust_cv: f64,
    /// Intervals used.
    pub n: usize,
}

/// Minimum intervals for a jitter reading.
pub const JITTER_MIN_INTERVALS: usize = 200;

/// Jitter from `(t_us, dx, dy)` motion reports of one device. Only intervals
/// where both reports carry at least 2 counts are used, so slow movement
/// (which naturally leaves polls empty) does not count as jitter.
pub fn jitter(reports: &[(u64, i32, i32)]) -> Option<Jitter> {
    let mut d: Vec<f64> = reports
        .windows(2)
        .filter(|w| w[0].1.unsigned_abs() + w[0].2.unsigned_abs() >= 2 && w[1].1.unsigned_abs() + w[1].2.unsigned_abs() >= 2)
        .map(|w| w[1].0.saturating_sub(w[0].0) as f64)
        .filter(|&x| x < 20_000.0)
        .collect();
    if d.len() < JITTER_MIN_INTERVALS {
        return None;
    }
    let n = d.len();
    let med = median(&mut d)?;
    if med <= 0.0 {
        return None;
    }
    let mut dev: Vec<f64> = d.iter().map(|x| (x - med).abs()).collect();
    let mad = median(&mut dev)?;
    Some(Jitter { median_us: med, robust_cv: 1.4826 * mad / med, n })
}

/// Floor for the block rule, so a very clean baseline does not flag ordinary noise.
pub const JITTER_FLOOR: f64 = 0.3;
/// Threshold without a baseline.
pub const JITTER_ABSOLUTE: f64 = 0.6;

/// Spec rule: a block is unusable when its jitter is more than 3× the
/// pre-test value (with a floor), or above an absolute limit when no
/// pre-test value exists.
pub fn jitter_is_chaotic(block: &Jitter, baseline: Option<&Jitter>) -> bool {
    let limit = match baseline {
        Some(b) => (3.0 * b.robust_cv).max(JITTER_FLOOR),
        None => JITTER_ABSOLUTE,
    };
    block.robust_cv > limit
}

#[cfg(test)]
mod check_tests {
    use super::*;

    /// Simple deterministic noise in [-1, 1].
    fn noise(i: u64, seed: u64) -> f64 {
        let mut x = i.wrapping_mul(6364136223846793005).wrapping_add(seed.wrapping_mul(1442695040888963407));
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51afd7ed558ccd);
        x ^= x >> 33;
        (x % 20001) as f64 / 10000.0 - 1.0
    }

    /// `secs` of fast circles at `hz` with timing noise `jit` (fraction of the
    /// interval), dropping every `drop_every`-th poll, `counts` per report.
    fn stream(hz: f64, secs: f64, jit: f64, drop_every: u64, counts: i32, seed: u64) -> Vec<Report> {
        let t = 1e9 / hz;
        let n = (secs * hz) as u64;
        let mut out = Vec::new();
        for i in 0..n {
            if drop_every > 0 && i % drop_every == drop_every - 1 {
                continue;
            }
            let ts = (i as f64 * t + jit * t * noise(i, seed)).max(0.0) as u64 + 1_000_000;
            out.push(Report { t_ns: ts, dx: counts, dy: counts / 2, batch: 1 });
        }
        out.sort_by_key(|r| r.t_ns);
        out
    }

    #[test]
    fn standard_rates_are_characterised() {
        for hz in [125u32, 500, 1000, 2000, 4000, 8000] {
            let r = check(&stream(hz as f64, 5.0, 0.05, 0, 6, hz as u64), Some(hz)).unwrap();
            assert_eq!(r.standard_hz, Some(hz), "{hz}");
            assert!((r.effective_hz - hz as f64).abs() / (hz as f64) < 0.03, "{hz}: {}", r.effective_hz);
            assert_eq!(r.verdict, Verdict::Good, "{hz}: {:?}", r.findings);
            assert!(!r.gimmick_suspect);
            let ih = r.interval_hz.unwrap();
            assert!((ih - hz as f64).abs() / (hz as f64) < 0.05, "{hz}: {ih}");
        }
    }

    #[test]
    fn unknown_claim_reports_standard() {
        let r = check(&stream(1000.0, 4.0, 0.03, 0, 5, 1), None).unwrap();
        assert_eq!(r.standard_hz, Some(1000));
        assert_eq!(r.verdict, Verdict::Good);
    }

    #[test]
    fn gimmick_8k_unstable_is_flagged() {
        // Nominal 8000, but reports come at an uneven ~2500 Hz.
        let r = check(&stream(2500.0, 5.0, 0.45, 0, 8, 7), Some(8000)).unwrap();
        assert_eq!(r.verdict, Verdict::Problem);
        assert!(r.gimmick_suspect, "{:?}", r.findings);
    }

    #[test]
    fn steady_lower_rate_is_reported_as_such() {
        let r = check(&stream(2000.0, 5.0, 0.03, 0, 8, 3), Some(8000)).unwrap();
        assert_eq!(r.verdict, Verdict::Problem);
        assert!(r.findings[0].title.contains("Steady at 2000 Hz"), "{:?}", r.findings[0]);
        assert!(r.gimmick_suspect);
    }

    #[test]
    fn slow_movement_is_not_blamed_on_the_mouse() {
        // 8000 set, but every report carries 1 count and only ~3000 arrive.
        let r = check(&stream(3000.0, 5.0, 0.3, 0, 1, 5).iter().map(|r| Report { dx: 1, dy: 0, ..*r }).collect::<Vec<_>>(), Some(8000))
            .unwrap();
        assert!(r.claim_unconfirmed);
        assert!(!r.gimmick_suspect);
        assert_ne!(r.verdict, Verdict::Problem, "{:?}", r.findings);
    }

    #[test]
    fn jittery_timing_is_flagged() {
        let r = check(&stream(1000.0, 5.0, 0.9, 0, 6, 9), Some(1000)).unwrap();
        assert_eq!(r.skipped_frac, None, "skips are not separable from heavy jitter");
        assert!(r.findings.iter().any(|f| f.title.contains("Uneven")), "{:?}", r.findings);
        assert_ne!(r.verdict, Verdict::Good);
    }

    #[test]
    fn drops_are_counted() {
        let r = check(&stream(1000.0, 5.0, 0.02, 7, 6, 11), Some(1000)).unwrap();
        let sk = r.skipped_frac.unwrap();
        assert!((sk - 1.0 / 6.0).abs() < 0.03, "{sk}");
        assert!(r.findings.iter().any(|f| f.source == Source::Connection && f.level == Verdict::Problem));
    }

    #[test]
    fn batched_delivery_uses_counts_and_blames_the_system() {
        // 8000 Hz stream read in groups of 4: same arrival time per group.
        let mut s = stream(8000.0, 5.0, 0.02, 0, 6, 13);
        for c in s.chunks_mut(4) {
            let t = c.last().unwrap().t_ns;
            for r in c.iter_mut() {
                r.t_ns = t;
                r.batch = 4;
            }
        }
        let r = check(&s, Some(8000)).unwrap();
        assert!((r.effective_hz - 8000.0).abs() < 300.0, "{}", r.effective_hz);
        assert!(r.bunched_frac > 0.6);
        assert_eq!(r.findings[0].level, Verdict::Good, "rate still delivered: {:?}", r.findings[0]);
        assert!(r.findings.iter().any(|f| f.source == Source::System));
        assert!(!r.gimmick_suspect);
        // Same stream at a lower effective rate: the PC is blamed, not the mouse.
        let thin: Vec<Report> = s.iter().enumerate().filter(|(i, _)| i % 2 == 0).map(|(_, r)| *r).collect();
        let r = check(&thin, Some(8000)).unwrap();
        assert_eq!(r.findings[0].source, Source::System);
        assert!(!r.gimmick_suspect);
    }

    #[test]
    fn not_measurable_cases() {
        assert_eq!(check(&stream(1000.0, 0.1, 0.0, 0, 6, 1), None), Err(NotMeasurable::TooFewReports(100)));
        // Long but with a pause every 60 ms: no complete windows.
        let s: Vec<Report> =
            (0..2000u64).map(|i| Report { t_ns: (i / 50) * 200_000_000 + (i % 50) * 1_000_000, dx: 3, dy: 0, batch: 1 }).collect();
        assert_eq!(check(&s, None), Err(NotMeasurable::TooLittleFastMovement));
    }

    #[test]
    fn jitter_rule() {
        let clean: Vec<(u64, i32, i32)> = (0..2000u64).map(|i| (i * 1000 + (noise(i, 1) * 30.0 + 30.0) as u64, 3, 1)).collect();
        let j = jitter(&clean).unwrap();
        assert!(j.robust_cv < 0.05, "{j:?}");
        let chaotic: Vec<(u64, i32, i32)> = (0..2000u64)
            .scan(0u64, |t, i| {
                *t += (1000.0 * (1.0 + 0.9 * noise(i, 2))) as u64;
                Some((*t, 3, 1))
            })
            .collect();
        let c = jitter(&chaotic).unwrap();
        assert!(jitter_is_chaotic(&c, Some(&j)));
        assert!(!jitter_is_chaotic(&j, Some(&j)));
        assert!(jitter_is_chaotic(&c, None));
        // Slow movement (1-count reports) is ignored entirely.
        let slow: Vec<(u64, i32, i32)> = (0..2000u64).map(|i| (i * 7000, 1, 0)).collect();
        assert!(jitter(&slow).is_none());
    }
}
