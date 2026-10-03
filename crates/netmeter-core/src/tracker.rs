use crate::config::{Config, Plan};
use crate::model::Traffic;
use crate::sampler::{Sampler, Tick};
use crate::stats::{self, Range};
use crate::store::Store;
use anyhow::Result;
use chrono::Local;
use std::time::{Duration, Instant};

/// Smoothing window for the live rate.
const RATE_WINDOW_MS: u64 = 5_000;
/// How much history to keep.
const RETENTION_SECS: i64 = 90 * stats::DAY;
/// How often to prune old rows while running.
const PRUNE_EVERY: Duration = Duration::from_secs(6 * 3_600);

/// Ties the sampler and the store together: sample, persist, query.
pub struct Tracker {
    config: Config,
    sampler: Sampler,
    store: Store,
    last_prune: Instant,
}

impl Tracker {
    /// Open using the default database path.
    pub fn open(config: Config) -> Result<Self> {
        let store = Store::open(&Config::db_path()?)?;
        Self::with_store(config, store)
    }

    pub fn with_store(config: Config, store: Store) -> Result<Self> {
        let mut tracker = Self {
            config,
            sampler: Sampler::new(RATE_WINDOW_MS),
            store,
            last_prune: Instant::now(),
        };
        tracker.prune()?;
        Ok(tracker)
    }

    /// Take one sample, persisting any completed minute bucket.
    pub fn tick(&mut self) -> Result<Tick> {
        let tick = self.sampler.tick();
        if let Some((ts, buckets)) = &tick.finalized {
            self.store.add_usage(*ts, buckets)?;
        }
        if self.last_prune.elapsed() >= PRUNE_EVERY {
            self.prune()?;
        }
        Ok(tick)
    }

    /// Flush the partial bucket on shutdown.
    pub fn shutdown(&mut self) -> Result<()> {
        if let Some((ts, buckets)) = self.sampler.flush() {
            self.store.add_usage(ts, &buckets)?;
        }
        Ok(())
    }

    fn prune(&mut self) -> Result<()> {
        self.store
            .prune_before(stats::unix_now() - RETENTION_SECS)?;
        self.last_prune = Instant::now();
        Ok(())
    }

    pub fn report(&self, range: Range, iface: Option<&str>) -> Result<Traffic> {
        let (from, to) = range.resolve(Local::now(), &self.config.plan);
        self.store.query_range(from, to, iface)
    }

    pub fn series(
        &self,
        range: Range,
        iface: Option<&str>,
        bucket_secs: i64,
    ) -> Result<Vec<(i64, Traffic)>> {
        let (from, to) = range.resolve(Local::now(), &self.config.plan);
        self.store.query_series(from, to, iface, bucket_secs)
    }

    /// Usage so far in the current billing cycle.
    pub fn cycle_usage(&self) -> Result<Traffic> {
        self.report(Range::BillingCycle, None)
    }

    /// Totals for a range, grouped per interface, busiest first.
    pub fn per_interface(&self, range: Range) -> Result<Vec<(String, Traffic)>> {
        let (from, to) = range.resolve(Local::now(), &self.config.plan);
        self.store.query_by_interface(from, to)
    }

    /// Traffic so far in the minute still in progress, if any.
    pub fn current_bucket(&self) -> Option<(i64, Traffic)> {
        self.sampler.current_bucket()
    }

    /// Start of the current billing cycle, as unix seconds.
    pub fn cycle_window_start(&self) -> i64 {
        stats::cycle_start(Local::now(), &self.config.plan).timestamp()
    }

    /// Highest alert level already reported for a cycle: `(cycle_start, level)`.
    pub fn alert_state(&self) -> Result<Option<(i64, f64)>> {
        let cycle = self.store.meta_get("alert_cycle")?;
        let level = self.store.meta_get("alert_level")?;
        let cycle = cycle.and_then(|value| value.parse::<i64>().ok());
        let level = level.and_then(|value| value.parse::<f64>().ok());
        Ok(cycle.zip(level))
    }

    pub fn record_alert(&self, cycle_start: i64, level: f64) -> Result<()> {
        self.store
            .meta_set("alert_cycle", &cycle_start.to_string())?;
        self.store.meta_set("alert_level", &level.to_string())
    }

    /// Swap in a config the user just saved.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn plan(&self) -> &Plan {
        &self.config.plan
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn interfaces(&self) -> Vec<String> {
        self.sampler.interface_names()
    }

    pub fn raw_totals(&self) -> Vec<(String, Traffic)> {
        self.sampler.raw_totals()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_the_configured_plan_for_cycle_ranges() {
        let store = Store::open_in_memory().unwrap();
        store
            .add_usage(
                stats::unix_now() - 60,
                &[("en0".to_string(), Traffic::new(500, 100))]
                    .into_iter()
                    .collect(),
            )
            .unwrap();

        let tracker = Tracker::with_store(Config::default(), store).unwrap();
        let today = tracker.report(Range::Today, None).unwrap();
        assert_eq!(today, Traffic::new(500, 100));

        let only_en6 = tracker.report(Range::Today, Some("en6")).unwrap();
        assert_eq!(only_en6, Traffic::ZERO);
    }
}
