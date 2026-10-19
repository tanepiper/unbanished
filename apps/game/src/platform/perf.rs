//! Frame profiler for finding hitches. Off unless `BANISHED_PROFILE` is set (native) or the page
//! has `?profile` (browser); when off, every call is a flag check.
//!
//! Each frame is split into named phases ([`scope`]). A frame much longer than usual is
//! reported as a hitch with its slowest phases. Every 600 frames (10 s at 60 fps) a summary
//! gives, per phase, the median, 95th percentile and worst time, and how often it ran.
//!
//! - `BANISHED_PROFILE=1`: hitches and summaries in the log.
//! - `BANISHED_PROFILE=hitches`: hitches only.
//!
//! Nothing here changes the simulation: it only reads the clock.

use std::cell::RefCell;
use std::collections::BTreeMap;

/// A frame this many times the recent median frame, and at least [`HITCH_MIN_MS`], is a hitch.
const HITCH_FACTOR: f64 = 2.0;
const HITCH_MIN_MS: f64 = 25.0;
const SUMMARY_FRAMES: usize = 600;
/// Recent frame times kept for the median.
const RECENT: usize = 120;

#[derive(Default)]
struct Phase {
    /// Time in this phase during the current frame (a phase can run several times a frame).
    frame_ms: f64,
    /// Every frame's time in this phase since the last summary.
    samples: Vec<f64>,
    calls: u64,
}

#[derive(Default)]
struct Profiler {
    on: bool,
    summaries: bool,
    frame_start: Option<f64>,
    frame: u64,
    recent: Vec<f64>,
    phases: BTreeMap<&'static str, Phase>,
    frames: Vec<f64>,
    hitches: u64,
}

thread_local! {
    static PROFILER: RefCell<Profiler> = RefCell::new(Profiler::new());
}

impl Profiler {
    fn new() -> Profiler {
        let mode = setting();
        Profiler {
            on: mode.is_some(),
            summaries: mode.as_deref() != Some("hitches"),
            ..Default::default()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn setting() -> Option<String> {
    std::env::var("BANISHED_PROFILE").ok().filter(|v| v != "0")
}

#[cfg(target_arch = "wasm32")]
fn setting() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params
        .get("profile")
        .map(|v| if v.is_empty() { "1".into() } else { v })
}

/// Milliseconds from a fixed point (monotonic).
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

/// Whether profiling is on.
pub fn enabled() -> bool {
    PROFILER.with(|p| p.borrow().on)
}

/// Start a frame.
pub fn frame_begin() {
    PROFILER.with(|p| {
        let mut p = p.borrow_mut();
        if p.on {
            p.frame_start = Some(now_ms());
        }
    });
}

/// End a frame: report a hitch, and a summary every [`SUMMARY_FRAMES`].
pub fn frame_end() {
    PROFILER.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on {
            return;
        }
        let Some(start) = p.frame_start.take() else {
            return;
        };
        let now = now_ms();
        let total = now - start;
        p.frame += 1;
        let median = median(&p.recent);
        if p.recent.len() >= RECENT / 2 && total > HITCH_MIN_MS && total > median * HITCH_FACTOR {
            p.hitches += 1;
            let mut worst: Vec<(&str, f64)> = p
                .phases
                .iter()
                .filter(|(_, ph)| ph.frame_ms > 0.5)
                .map(|(n, ph)| (*n, ph.frame_ms))
                .collect();
            worst.sort_by(|a, b| b.1.total_cmp(&a.1));
            let parts: Vec<String> = worst
                .iter()
                .take(6)
                .map(|(n, ms)| format!("{n} {ms:.1}"))
                .collect();
            log::warn!(
                "hitch: frame {} took {total:.1} ms (median {median:.1}): {}",
                p.frame,
                parts.join(", ")
            );
        }
        if p.recent.len() == RECENT {
            p.recent.remove(0);
        }
        p.recent.push(total);
        p.frames.push(total);
        for ph in p.phases.values_mut() {
            let ms = std::mem::take(&mut ph.frame_ms);
            ph.samples.push(ms);
        }
        if p.frames.len() >= SUMMARY_FRAMES {
            if p.summaries {
                summary(&mut p);
            }
            p.frames.clear();
            p.hitches = 0;
            for ph in p.phases.values_mut() {
                ph.samples.clear();
                ph.calls = 0;
            }
        }
    });
}

fn summary(p: &mut Profiler) {
    let mut frames = p.frames.clone();
    let (p50, p95, max) = stats(&mut frames);
    log::info!(
        "profile: {} frames, frame p50 {p50:.1} p95 {p95:.1} max {max:.1} ms, {} hitches",
        frames.len(),
        p.hitches
    );
    let mut rows: Vec<(&str, f64, f64, f64, u64)> = p
        .phases
        .iter_mut()
        .map(|(n, ph)| {
            let (a, b, c) = stats(&mut ph.samples);
            (*n, a, b, c, ph.calls)
        })
        .collect();
    rows.sort_by(|a, b| b.3.total_cmp(&a.3));
    for (name, p50, p95, max, calls) in rows.into_iter().filter(|r| r.3 >= 0.05) {
        log::info!("  {name:<28} p50 {p50:6.2}  p95 {p95:6.2}  max {max:7.2} ms  ({calls} calls)");
    }
}

fn median(v: &[f64]) -> f64 {
    let mut v = v.to_vec();
    stats(&mut v).0
}

/// Median, 95th percentile and maximum.
fn stats(v: &mut [f64]) -> (f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    (at(0.5), at(0.95), v[v.len() - 1])
}

/// Times the rest of the enclosing block as phase `name`.
#[must_use]
pub fn scope(name: &'static str) -> Scope {
    Scope {
        name,
        start: if enabled() { Some(now_ms()) } else { None },
    }
}

pub struct Scope {
    name: &'static str,
    start: Option<f64>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let ms = now_ms() - start;
            PROFILER.with(|p| {
                let mut p = p.borrow_mut();
                let ph = p.phases.entry(self.name).or_default();
                ph.frame_ms += ms;
                ph.calls += 1;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_give_median_p95_and_max() {
        let mut v: Vec<f64> = (1..=100).map(f64::from).collect();
        let (p50, p95, max) = stats(&mut v);
        assert_eq!((p50.round(), p95.round(), max), (51.0, 95.0, 100.0));
    }
}
