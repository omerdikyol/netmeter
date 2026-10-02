//! Per-application network usage.
//!
//! macOS ships `nettop`, which reports byte counters per process. We poll it
//! rather than displaying it, so we get deltas we can attribute and total.
//!
//! Node: this is macOS-only. Windows and Linux would each need their own
//! mechanism (ETW, or eBPF/`nethogs`), so elsewhere the panel simply reports
//! that per-app usage is unavailable.

use anyhow::{Context, Result};
use netmeter_core::sampler::advance;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const NETTOP: &str = "/usr/bin/nettop";
/// How many apps the panel shows.
const MAX_APPS: usize = 6;

#[derive(Debug, Clone, Serialize)]
pub struct AppUsage {
    pub name: String,
    pub rx: u64,
    pub tx: u64,
    pub total: u64,
    #[serde(rename = "rateRx")]
    pub rate_rx: f64,
    #[serde(rename = "rateTx")]
    pub rate_tx: f64,
}

impl AppUsage {
    fn new(name: String, rx: u64, tx: u64, rate_rx: f64, rate_tx: f64) -> Self {
        Self {
            name,
            rx,
            tx,
            total: rx.saturating_add(tx),
            rate_rx,
            rate_tx,
        }
    }
}

#[derive(Default)]
struct Entry {
    rx: u64,
    tx: u64,
    rate_rx: f64,
    rate_tx: f64,
}

#[derive(Default)]
struct Inner {
    /// Traffic accumulated since the app started, keyed by process name.
    apps: HashMap<String, Entry>,
    /// Last raw counters, keyed by `(process name, pid)`.
    prev: HashMap<(String, u32), (u64, u64)>,
    last_at: Option<Instant>,
    supported: bool,
    error: Option<String>,
}

/// Polls `nettop` on a worker thread and keeps a running per-app tally.
pub struct AppMonitor {
    inner: Arc<Mutex<Inner>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl AppMonitor {
    pub fn start(interval: Duration) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            supported: cfg!(target_os = "macos"),
            ..Inner::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));

        let worker = if cfg!(target_os = "macos") {
            let (inner, stop) = (Arc::clone(&inner), Arc::clone(&stop));
            Some(thread::spawn(move || poll_loop(inner, stop, interval)))
        } else {
            None
        };

        Self {
            inner,
            stop,
            worker,
        }
    }

    /// `(per-app usage is available, busiest apps since launch)`.
    pub fn snapshot(&self) -> (bool, Vec<AppUsage>) {
        snapshot(&lock(&self.inner))
    }

    /// Why per-app sampling is unavailable, if it failed.
    pub fn error(&self) -> Option<String> {
        lock(&self.inner).error.clone()
    }
}

/// Read the busiest apps out of the shared state.
fn snapshot(inner: &Inner) -> (bool, Vec<AppUsage>) {
    let mut apps: Vec<AppUsage> = inner
        .apps
        .iter()
        .filter(|(_, entry)| entry.rx + entry.tx > 0)
        .map(|(name, entry)| {
            AppUsage::new(
                name.clone(),
                entry.rx,
                entry.tx,
                entry.rate_rx,
                entry.rate_tx,
            )
        })
        .collect();
    apps.sort_by_key(|app| std::cmp::Reverse(app.total));
    apps.truncate(MAX_APPS);
    (inner.supported, apps)
}

impl Drop for AppMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock only means a previous sample panicked; keep going.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn poll_loop(inner: Arc<Mutex<Inner>>, stop: Arc<AtomicBool>, interval: Duration) {
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let started = Instant::now();
        match sample() {
            Ok(rows) => record(&inner, rows),
            Err(err) => {
                let mut guard = lock(&inner);
                guard.supported = false;
                guard.error = Some(format!("{err:#}"));
                return;
            }
        }

        // Sleep in small steps so shutdown stays responsive.
        if let Some(remaining) = interval.checked_sub(started.elapsed()) {
            let mut slept = Duration::ZERO;
            while slept < remaining {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let step = Duration::from_millis(200).min(remaining - slept);
                thread::sleep(step);
                slept += step;
            }
        }
    }
}

/// A process identity: executable name plus pid.
type ProcessKey = (String, u32);
/// Bytes in and out for one process at one instant.
type Counters = (u64, u64);
/// One `nettop` sample.
type Sample = Vec<(ProcessKey, Counters)>;

/// One `nettop` sample: `(name, pid) -> (bytes in, bytes out)`.
fn sample() -> Result<Sample> {
    let output = std::process::Command::new(NETTOP)
        .args(["-n", "-x", "-P", "-l", "1", "-J", "bytes_in,bytes_out"])
        .output()
        .with_context(|| format!("failed to run {NETTOP}"))?;
    if !output.status.success() {
        anyhow::bail!("{NETTOP} exited with {}", output.status);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut rows = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(label), Some(rx), Some(tx)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Some(rx), Some(tx)) = (rx.parse::<u64>().ok(), tx.parse::<u64>().ok()) else {
            continue;
        };
        let (name, pid) = split_label(label);
        rows.push(((name, pid), (rx, tx)));
    }
    Ok(rows)
}

/// `mDNSResponder.430` -> `("mDNSResponder", 430)`.
///
/// Some names contain dots themselves, so only treat the last segment as a pid
/// when it really is one.
fn split_label(label: &str) -> (String, u32) {
    match label.rsplit_once('.') {
        Some((name, pid)) => match pid.parse::<u32>() {
            Ok(pid) => (name.to_string(), pid),
            Err(_) => (label.to_string(), 0),
        },
        None => (label.to_string(), 0),
    }
}

fn record(inner: &Arc<Mutex<Inner>>, rows: Sample) {
    let now = Instant::now();
    let mut guard = lock(inner);
    let elapsed = guard
        .last_at
        .map(|last| now.duration_since(last).as_secs_f64())
        .unwrap_or(0.0);
    guard.last_at = Some(now);

    let mut next = HashMap::with_capacity(rows.len());
    let mut deltas: HashMap<String, (u64, u64)> = HashMap::new();
    for (key, (rx, tx)) in rows {
        let mut delta_rx = 0;
        let mut delta_tx = 0;
        if let Some(&(prev_rx, prev_tx)) = guard.prev.get(&key) {
            delta_rx = advance(Some(prev_rx), rx);
            delta_tx = advance(Some(prev_tx), tx);
        }
        if delta_rx > 0 || delta_tx > 0 {
            let entry = deltas.entry(key.0.clone()).or_insert((0, 0));
            entry.0 += delta_rx;
            entry.1 += delta_tx;
        }
        next.insert(key, (rx, tx));
    }
    guard.prev = next;

    // Rates are momentary, so clear them before applying this sample's deltas.
    for entry in guard.apps.values_mut() {
        entry.rate_rx = 0.0;
        entry.rate_tx = 0.0;
    }
    let seconds = elapsed.max(0.001);
    for (name, (delta_rx, delta_tx)) in deltas {
        let entry = guard.apps.entry(name).or_default();
        entry.rx = entry.rx.saturating_add(delta_rx);
        entry.tx = entry.tx.saturating_add(delta_tx);
        entry.rate_rx = delta_rx as f64 / seconds;
        entry.rate_tx = delta_tx as f64 / seconds;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_split_into_name_and_pid() {
        assert_eq!(
            split_label("mDNSResponder.430"),
            ("mDNSResponder".into(), 430)
        );
        assert_eq!(
            split_label("Google Chrome Helper.12"),
            ("Google Chrome Helper".into(), 12)
        );
        // A dotted name with no pid keeps the whole label.
        assert_eq!(
            split_label("launchd.develop"),
            ("launchd.develop".into(), 0)
        );
        assert_eq!(split_label("nettop"), ("nettop".into(), 0));
    }

    #[test]
    fn totals_accumulate_from_deltas() {
        let inner = Arc::new(Mutex::new(Inner {
            supported: true,
            ..Inner::default()
        }));
        let key = ("Chrome".to_string(), 1u32);

        // First sample only establishes a baseline.
        record(&inner, vec![(key.clone(), (1_000, 500))]);
        let (_, apps) = snapshot(&lock(&inner));
        assert!(apps.is_empty(), "baseline sample should not count");

        // Second sample adds the difference.
        record(&inner, vec![(key.clone(), (1_600, 700))]);
        let (_, apps) = snapshot(&lock(&inner));
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Chrome");
        assert_eq!(apps[0].rx, 600);
        assert_eq!(apps[0].tx, 200);
        assert_eq!(apps[0].total, 800);
    }

    #[test]
    fn counter_reset_does_not_produce_a_negative_total() {
        let inner = Arc::new(Mutex::new(Inner {
            supported: true,
            ..Inner::default()
        }));
        let key = ("App".to_string(), 7u32);
        record(&inner, vec![(key.clone(), (5_000, 5_000))]);
        // The pid was reused: counters dropped back to near zero.
        record(&inner, vec![(key, (10, 10))]);
        let (_, apps) = snapshot(&lock(&inner));
        assert_eq!(apps[0].rx, 10);
        assert_eq!(apps[0].tx, 10);
    }
}
