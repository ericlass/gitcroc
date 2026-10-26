//! Frame-time statistics, comparable with `bench-ui`'s `scroll-frame` and
//! `scroll-blank`.

use std::fmt::Write as _;
use std::time::Duration;

pub struct Scenario {
    pub name: &'static str,
    /// Time between consecutive frames, in ms.
    pub frames: Vec<f32>,
    /// CPU time of each frame, in ms.
    pub cpu: Vec<f32>,
    pub longest_blank: Duration,
    pub distance_px: f32,
    pub reached_end: bool,
    /// Rows or lines the list held at the end.
    pub items: usize,
}

pub struct Summary {
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
    pub fps: f32,
    /// Share of frames that took over 1.5 refresh intervals.
    pub janky: f32,
}

pub fn summarize(frames: &[f32], refresh_ms: f32) -> Option<Summary> {
    if frames.is_empty() {
        return None;
    }
    let mut sorted = frames.to_vec();
    sorted.sort_by(f32::total_cmp);
    let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q).round() as usize];
    let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
    let janky = frames.iter().filter(|&&ms| ms > refresh_ms * 1.5).count();
    Some(Summary {
        p50: at(0.5),
        p95: at(0.95),
        p99: at(0.99),
        max: at(1.0),
        fps: 1000.0 / mean,
        janky: janky as f32 / frames.len() as f32 * 100.0,
    })
}

/// One JSON object per scenario plus the run's context, by hand to keep the
/// spike's dependencies to GPUI and gitcroc-git.
pub fn json(scenarios: &[Scenario], context: &[(&str, String)], refresh_ms: f32) -> String {
    let mut out = String::from("{\n");
    for (key, value) in context {
        let _ = writeln!(out, "  \"{key}\": {value},");
    }
    out.push_str("  \"scenarios\": [\n");
    for (index, scenario) in scenarios.iter().enumerate() {
        let comma = if index + 1 < scenarios.len() { "," } else { "" };
        let cpu = summarize(&scenario.cpu, refresh_ms);
        let cpu_at = |pick: fn(&Summary) -> f32| cpu.as_ref().map_or(0.0, pick);
        match summarize(&scenario.frames, refresh_ms) {
            Some(s) => {
                let _ = writeln!(
                    out,
                    "    {{\"name\": \"{}\", \"frames\": {}, \"fps\": {:.1}, \"p50_ms\": {:.2}, \"p95_ms\": {:.2}, \
                     \"p99_ms\": {:.2}, \"max_ms\": {:.2}, \"janky_pct\": {:.2}, \"cpu_p50_ms\": {:.2}, \"cpu_p95_ms\": {:.2}, \"cpu_max_ms\": {:.2}, \"longest_blank_ms\": {:.1}, \
                     \"distance_px\": {:.0}, \"items\": {}, \"reached_end\": {}}}{comma}",
                    scenario.name,
                    scenario.frames.len(),
                    s.fps,
                    s.p50,
                    s.p95,
                    s.p99,
                    s.max,
                    s.janky,
                    cpu_at(|c| c.p50),
                    cpu_at(|c| c.p95),
                    cpu_at(|c| c.max),
                    scenario.longest_blank.as_secs_f32() * 1000.0,
                    scenario.distance_px,
                    scenario.items,
                    scenario.reached_end,
                );
            }
            None => {
                let _ = writeln!(out, "    {{\"name\": \"{}\", \"frames\": 0}}{comma}", scenario.name);
            }
        }
    }
    out.push_str("  ]\n}");
    out
}
