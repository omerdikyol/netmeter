use crate::model::Traffic;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS usage (
    ts       INTEGER NOT NULL,
    iface    TEXT    NOT NULL,
    rx_bytes INTEGER NOT NULL,
    tx_bytes INTEGER NOT NULL,
    PRIMARY KEY (ts, iface)
);
CREATE INDEX IF NOT EXISTS idx_usage_ts ON usage(ts);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const UPSERT_USAGE: &str = r#"
INSERT INTO usage (ts, iface, rx_bytes, tx_bytes) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(ts, iface) DO UPDATE SET
    rx_bytes = rx_bytes + excluded.rx_bytes,
    tx_bytes = tx_bytes + excluded.tx_bytes
"#;

const SUM_ALL: &str =
    "SELECT COALESCE(SUM(rx_bytes), 0), COALESCE(SUM(tx_bytes), 0) FROM usage WHERE ts >= ?1 AND ts < ?2";
const SUM_IFACE: &str = "SELECT COALESCE(SUM(rx_bytes), 0), COALESCE(SUM(tx_bytes), 0) FROM usage WHERE ts >= ?1 AND ts < ?2 AND iface = ?3";

/// SQLite-backed usage history.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        let store = Self { conn };
        store
            .conn
            .execute_batch(SCHEMA)
            .context("failed to apply schema")?;
        Ok(store)
    }

    /// Add byte counts to a minute bucket for each interface.
    pub fn add_usage(&self, ts: i64, buckets: &HashMap<String, Traffic>) -> Result<()> {
        if buckets.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(UPSERT_USAGE)?;
            for (iface, traffic) in buckets {
                stmt.execute(params![ts, iface, traffic.rx as i64, traffic.tx as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Sum of traffic in `[from, to)`, optionally limited to one interface.
    pub fn query_range(&self, from: i64, to: i64, iface: Option<&str>) -> Result<Traffic> {
        let row = |rx: i64, tx: i64| Traffic::new(rx as u64, tx as u64);
        let traffic = match iface {
            Some(iface) => self
                .conn
                .query_row(SUM_IFACE, params![from, to, iface], |r| {
                    Ok(row(r.get(0)?, r.get(1)?))
                })?,
            None => self.conn.query_row(SUM_ALL, params![from, to], |r| {
                Ok(row(r.get(0)?, r.get(1)?))
            })?,
        };
        Ok(traffic)
    }

    /// Sums grouped into `bucket_secs` buckets, for charts.
    pub fn query_series(
        &self,
        from: i64,
        to: i64,
        iface: Option<&str>,
        bucket_secs: i64,
    ) -> Result<Vec<(i64, Traffic)>> {
        let bucket = bucket_secs.max(1);
        let sql = match iface {
            Some(_) => {
                "SELECT (ts / ?1) * ?1 AS b, SUM(rx_bytes), SUM(tx_bytes) FROM usage \
                        WHERE ts >= ?2 AND ts < ?3 AND iface = ?4 GROUP BY b ORDER BY b"
            }
            None => {
                "SELECT (ts / ?1) * ?1 AS b, SUM(rx_bytes), SUM(tx_bytes) FROM usage \
                     WHERE ts >= ?2 AND ts < ?3 GROUP BY b ORDER BY b"
            }
        };

        let map_row = |r: &rusqlite::Row| -> rusqlite::Result<(i64, Traffic)> {
            Ok((
                r.get(0)?,
                Traffic::new(r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64),
            ))
        };
        let mut stmt = self.conn.prepare(sql)?;
        let series = match iface {
            Some(iface) => stmt
                .query_map(params![bucket, from, to, iface], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => stmt
                .query_map(params![bucket, from, to], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        Ok(series)
    }

    /// Interfaces seen in stored history.
    pub fn interfaces(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT iface FROM usage ORDER BY iface")?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(names)
    }

    /// Delete everything older than `ts`. Returns the number of rows removed.
    pub fn prune_before(&self, ts: i64) -> Result<usize> {
        let removed = self
            .conn
            .execute("DELETE FROM usage WHERE ts < ?1", params![ts])?;
        Ok(removed)
    }

    /// Forget all recorded usage.
    pub fn reset(&self) -> Result<()> {
        self.conn.execute("DELETE FROM usage", [])?;
        Ok(())
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        Ok(value)
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buckets(entries: &[(&str, u64, u64)]) -> HashMap<String, Traffic> {
        entries
            .iter()
            .map(|(name, rx, tx)| ((*name).to_string(), Traffic::new(*rx, *tx)))
            .collect()
    }

    #[test]
    fn sums_across_interfaces_and_time() {
        let store = Store::open_in_memory().unwrap();
        store.add_usage(60, &buckets(&[("en0", 100, 10)])).unwrap();
        store
            .add_usage(120, &buckets(&[("en0", 200, 20), ("en6", 1_000, 5)]))
            .unwrap();

        let all = store.query_range(0, 1_000, None).unwrap();
        assert_eq!(all, Traffic::new(1_300, 35));

        let only_en6 = store.query_range(0, 1_000, Some("en6")).unwrap();
        assert_eq!(only_en6, Traffic::new(1_000, 5));
    }

    #[test]
    fn range_is_from_inclusive_to_exclusive() {
        let store = Store::open_in_memory().unwrap();
        store.add_usage(100, &buckets(&[("en0", 1, 0)])).unwrap();
        store.add_usage(200, &buckets(&[("en0", 2, 0)])).unwrap();

        assert_eq!(store.query_range(100, 200, None).unwrap().rx, 1);
        assert_eq!(store.query_range(100, 201, None).unwrap().rx, 3);
    }

    #[test]
    fn same_bucket_accumulates() {
        let store = Store::open_in_memory().unwrap();
        store.add_usage(60, &buckets(&[("en0", 100, 0)])).unwrap();
        store.add_usage(60, &buckets(&[("en0", 250, 0)])).unwrap();
        assert_eq!(store.query_range(0, 1_000, None).unwrap().rx, 350);
    }

    #[test]
    fn series_groups_into_buckets() {
        let store = Store::open_in_memory().unwrap();
        store.add_usage(0, &buckets(&[("en0", 10, 0)])).unwrap();
        store.add_usage(60, &buckets(&[("en0", 20, 0)])).unwrap();
        store.add_usage(120, &buckets(&[("en0", 30, 0)])).unwrap();

        let series = store.query_series(0, 300, None, 120).unwrap();
        assert_eq!(series.len(), 2);
        assert_eq!(series[0], (0, Traffic::new(30, 0)));
        assert_eq!(series[1], (120, Traffic::new(30, 0)));
    }

    #[test]
    fn prune_and_reset() {
        let store = Store::open_in_memory().unwrap();
        store.add_usage(10, &buckets(&[("en0", 1, 0)])).unwrap();
        store.add_usage(1_000, &buckets(&[("en0", 2, 0)])).unwrap();

        assert_eq!(store.prune_before(500).unwrap(), 1);
        assert_eq!(store.query_range(0, 10_000, None).unwrap().rx, 2);

        store.reset().unwrap();
        assert_eq!(store.query_range(0, 10_000, None).unwrap().rx, 0);
        assert!(store.interfaces().unwrap().is_empty());
    }

    #[test]
    fn meta_round_trips() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.meta_get("last_alert").unwrap(), None);
        store.meta_set("last_alert", "0.8").unwrap();
        store.meta_set("last_alert", "1.0").unwrap();
        assert_eq!(
            store.meta_get("last_alert").unwrap().as_deref(),
            Some("1.0")
        );
    }
}
