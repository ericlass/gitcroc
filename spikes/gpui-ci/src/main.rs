//! Throwaway spike (M6-03): the window of `lib.rs`.
//!
//!   gpui-ci-spike [--frames <n>] [--timeout <seconds>]
//!
//! With `--frames` it scrolls the list for `n` frames, prints one JSON line
//! with the frame times, the GPU adapter and the backend, then exits 0. It
//! exits 1 when no frame was presented within the timeout (default 120 s).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui_ci_spike::{FrameRun, Spike};
use gpui_kit::component::Theme;
use gpui_kit::*;

/// GPUI logs the adapter wgpu picked, with its backend, as one info line;
/// keep it, and pass warnings and errors to stderr.
static ADAPTER_LINE: Mutex<Option<String>> = Mutex::new(None);
static FINISHED: AtomicBool = AtomicBool::new(false);

struct Logger {
    verbose: bool,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        let message = record.args().to_string();
        if message.starts_with("Selected GPU adapter")
            && let Ok(mut line) = ADAPTER_LINE.lock()
        {
            *line = Some(message.clone());
        }
        if self.verbose || record.level() <= log::Level::Warn {
            eprintln!("[{} {}] {message}", record.level(), record.target());
        }
    }

    fn flush(&self) {}
}

struct Args {
    frames: Option<usize>,
    timeout: u64,
}

fn args() -> Result<Args, String> {
    let mut parsed = Args {
        frames: None,
        timeout: 120,
    };
    let mut rest = std::env::args().skip(1);
    while let Some(arg) = rest.next() {
        let mut number = |name: &str| {
            rest.next()
                .and_then(|n| n.parse::<u64>().ok())
                .ok_or(format!("{name} needs a number"))
        };
        match arg.as_str() {
            "--frames" => parsed.frames = Some(number("--frames")? as usize),
            "--timeout" => parsed.timeout = number("--timeout")?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(parsed)
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * q).round() as usize]
}

fn json_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn backend() -> String {
    if cfg!(target_os = "windows") {
        "Direct3D 11".into()
    } else if cfg!(target_os = "macos") {
        "Metal".into()
    } else {
        ADAPTER_LINE
            .lock()
            .ok()
            .and_then(|line| line.clone())
            .and_then(|line| {
                line.rsplit_once('(')
                    .map(|(_, rest)| rest.trim_end_matches(')').to_string())
            })
            .unwrap_or_else(|| "unknown".into())
    }
}

fn report(frames: &[f32], started: Instant, window: &Window) -> String {
    let mut sorted = frames.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mean = sorted.iter().sum::<f32>() / sorted.len().max(1) as f32;
    let specs = window.gpu_specs();
    let adapter = specs.as_ref().map_or_else(
        || {
            ADAPTER_LINE
                .lock()
                .ok()
                .and_then(|line| line.clone())
                .unwrap_or_default()
        },
        |specs| specs.device_name.clone(),
    );
    let (software, driver) =
        specs
            .as_ref()
            .map_or((String::from("null"), String::new()), |specs| {
                (
                    specs.is_software_emulated.to_string(),
                    format!("{} {}", specs.driver_name, specs.driver_info),
                )
            });
    let (p50, p95, p99, max) = if sorted.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (
            percentile(&sorted, 0.5),
            percentile(&sorted, 0.95),
            percentile(&sorted, 0.99),
            percentile(&sorted, 1.0),
        )
    };
    format!(
        "{{\"os\": {}, \"frames\": {}, \"seconds\": {:.2}, \"fps\": {:.1}, \"p50_ms\": {p50:.2}, \"p95_ms\": {p95:.2}, \
         \"p99_ms\": {p99:.2}, \"max_ms\": {max:.2}, \"adapter\": {}, \"software\": {software}, \"driver\": {}, \"backend\": {}}}",
        json_string(std::env::consts::OS),
        frames.len(),
        started.elapsed().as_secs_f32(),
        if mean > 0.0 { 1000.0 / mean } else { 0.0 },
        json_string(&adapter),
        json_string(driver.trim()),
        json_string(&backend()),
    )
}

fn main() {
    let args = match args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let verbose = std::env::var_os("SPIKE_VERBOSE").is_some();
    if log::set_boxed_logger(Box::new(Logger { verbose })).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let started = Instant::now();
    if args.frames.is_some() {
        let timeout = args.timeout;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout));
            if !FINISHED.load(Ordering::SeqCst) {
                eprintln!("no report within {timeout} s: the window presented too few frames");
                std::process::exit(1);
            }
        });
    }
    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1024.), px(768.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("gitcroc GPUI CI spike".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let run = args.frames.map(|target| {
            FrameRun::new(target, move |frames, window, _| {
                FINISHED.store(true, Ordering::SeqCst);
                println!("{}", report(&frames, started, window));
                std::process::exit(if frames.is_empty() { 1 } else { 0 });
            })
        });
        let opened = gpui_kit::open_window(options, cx, |window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
            cx.new(|_| Spike::new(run))
        });
        if let Err(error) = opened {
            eprintln!("cannot open the window: {error:#}");
            std::process::exit(1);
        }
    });
}
