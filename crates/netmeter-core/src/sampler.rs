use crate::model::{Rate, Traffic};
use crate::stats::{floor_minute, unix_now};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use sysinfo::Networks;

/// Bytes observed between two raw counter reads.
///
/// Interface counters restart on reboot or link flap, so a value that went
/// *down* means the counter reset: we count from zero rather than reporting a
/// negative delta. A `None` previous value means we have no baseline yet, so we
/// report nothing.
pub fn advance(prev: Option<u64>, now: u64) -> u64 {
    match prev {
        None => 0,
        Some(prev) if now >= prev => now - prev,
        Some(_) => now,
    }
}

/// A sliding window used to smooth the live transfer rate.
pub struct RateWindow {
    window_ms: u64,
    samples: VecDeque<(u64, Traffic)>,
}

impl RateWindow {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms: window_ms.max(1),
            samples: VecDeque::new(),
        }
    }

    /// Record the bytes seen since the previous sample at monotonic `t_ms`.
    pub fn push(&mut self, t_ms: u64, delta: Traffic) {
        self.samples.push_back((t_ms, delta));
        let cutoff = t_ms.saturating_sub(self.window_ms);
        while let Some(&(t, _)) = self.samples.front() {
            if t < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn rate(&self) -> Rate {
        if self.samples.len() < 2 {
            return Rate::default();
        }
        let total = self
            .samples
            .iter()
            .fold(Traffic::ZERO, |acc, (_, delta)| acc + *delta);
        let first = self.samples.front().expect("non-empty").0;
        let last = self.samples.back().expect("non-empty").0;
        let secs = (last.saturating_sub(first) as f64 / 1000.0).max(0.001);
        Rate {
            rx_per_sec: total.rx as f64 / secs,
            tx_per_sec: total.tx as f64 / secs,
        }
    }
}

/// The result of one sampling tick.
#[derive(Debug, Default)]
pub struct Tick {
    /// Combined rate across all live interfaces.
    pub rate: Rate,
    pub per_interface_rate: HashMap<String, Rate>,
    /// Bytes seen since the previous tick, per interface.
    pub deltas: HashMap<String, Traffic>,
    /// A completed minute bucket that should be persisted: `(bucket_start, usage)`.
    pub finalized: Option<(i64, HashMap<String, Traffic>)>,
}

/// Reads interface counters and aggregates them into minute buckets.
pub struct Sampler {
    networks: Networks,
    prev: HashMap<String, Traffic>,
    rates: HashMap<String, RateWindow>,
    start: Instant,
    window_ms: u64,
    bucket_start: i64,
    bucket: HashMap<String, Traffic>,
}

impl Sampler {
    pub fn new(window_ms: u64) -> Self {
        Self {
            networks: Networks::new_with_refreshed_list(),
            prev: HashMap::new(),
            rates: HashMap::new(),
            start: Instant::now(),
            window_ms: window_ms.max(1),
            bucket_start: floor_minute(unix_now()),
            bucket: HashMap::new(),
        }
    }

    pub fn tick(&mut self) -> Tick {
        self.networks.refresh(true);
        let now_ms = self.start.elapsed().as_millis() as u64;
        let this_minute = floor_minute(unix_now());

        let mut finalized = None;
        if this_minute != self.bucket_start {
            let done = std::mem::take(&mut self.bucket);
            if !done.is_empty() {
                finalized = Some((self.bucket_start, done));
            }
            self.bucket_start = this_minute;
        }

        let mut tick = Tick {
            finalized,
            ..Tick::default()
        };
        let mut live: Vec<String> = Vec::new();

        for (name, data) in self.networks.iter() {
            let now = Traffic::new(data.total_received(), data.total_transmitted());
            let prev = self.prev.insert(name.clone(), now);
            let delta = Traffic::new(
                advance(prev.map(|p| p.rx), now.rx),
                advance(prev.map(|p| p.tx), now.tx),
            );

            self.rates
                .entry(name.clone())
                .or_insert_with(|| RateWindow::new(self.window_ms))
                .push(now_ms, delta);
            live.push(name.clone());

            if !delta.is_zero() {
                *self.bucket.entry(name.clone()).or_default() += delta;
            }
            tick.deltas.insert(name.clone(), delta);
        }

        self.rates.retain(|name, _| live.contains(name));
        for name in &live {
            if let Some(rate) = self.rates.get(name).map(RateWindow::rate) {
                tick.rate.rx_per_sec += rate.rx_per_sec;
                tick.rate.tx_per_sec += rate.tx_per_sec;
                tick.per_interface_rate.insert(name.clone(), rate);
            }
        }

        tick
    }

    /// Return the in-progress minute bucket so it can be persisted on shutdown.
    pub fn flush(&mut self) -> Option<(i64, HashMap<String, Traffic>)> {
        let done = std::mem::take(&mut self.bucket);
        if done.is_empty() {
            None
        } else {
            Some((self.bucket_start, done))
        }
    }

    /// Traffic accumulated so far in the minute that is still in progress.
    pub fn current_bucket(&self) -> Option<(i64, Traffic)> {
        if self.bucket.is_empty() {
            return None;
        }
        let total = self
            .bucket
            .values()
            .fold(Traffic::ZERO, |acc, traffic| acc + *traffic);
        Some((self.bucket_start, total))
    }

    pub fn interface_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.networks.keys().cloned().collect();
        names.sort();
        names
    }

    /// Raw lifetime counters (since boot / interface reset), per interface.
    pub fn raw_totals(&self) -> Vec<(String, Traffic)> {
        let mut totals: Vec<(String, Traffic)> = self
            .prev
            .iter()
            .map(|(name, t)| (name.clone(), *t))
            .collect();
        totals.sort_by(|a, b| a.0.cmp(&b.0));
        totals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_reading_has_no_baseline() {
        assert_eq!(advance(None, 9_999), 0);
    }

    #[test]
    fn normal_delta_is_the_difference() {
        assert_eq!(advance(Some(1_000), 1_500), 500);
    }

    #[test]
    fn counter_reset_counts_from_zero() {
        // A reboot/flap makes the counter smaller; we count what we can see.
        assert_eq!(advance(Some(5_000), 120), 120);
        assert_eq!(advance(Some(5_000), 5_000), 0);
    }

    #[test]
    fn rate_window_averages_over_time() {
        let mut w = RateWindow::new(5_000);
        w.push(0, Traffic::new(1_000, 0));
        w.push(1_000, Traffic::new(1_000, 0));
        let rate = w.rate();
        assert!((rate.rx_per_sec - 2_000.0).abs() < 1e-6);
    }

    #[test]
    fn rate_window_drops_stale_samples() {
        let mut w = RateWindow::new(5_000);
        w.push(0, Traffic::new(1_000, 0));
        w.push(6_000, Traffic::new(1_000, 0));
        // The t=0 sample is outside the window, leaving a single point -> no rate.
        assert_eq!(w.rate(), Rate::default());
    }

    #[test]
    fn single_sample_has_no_rate() {
        let mut w = RateWindow::new(5_000);
        w.push(0, Traffic::new(1_000, 0));
        assert_eq!(w.rate(), Rate::default());
    }
}
