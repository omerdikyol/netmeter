use anyhow::{bail, Context, Result};
use chrono::{Local, NaiveDate, TimeZone};
use clap::{Args, ValueEnum};
use netmeter_core::config::Config;
use netmeter_core::format::{format_bytes, format_rate};
use netmeter_core::model::Traffic;
use netmeter_core::sampler::Sampler;
use netmeter_core::stats::Range;
use netmeter_core::store::Store;
use serde::Serialize;
use std::thread::sleep;
use std::time::Duration;

/// Smoothing window for the live rate shown by `sample`.
const RATE_WINDOW_MS: u64 = 5_000;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum RangeArg {
    LastHour,
    Today,
    Last24h,
    Last7days,
    Thismonth,
    Cycle,
}

impl RangeArg {
    fn to_range(self) -> Range {
        match self {
            RangeArg::LastHour => Range::LastHour,
            RangeArg::Today => Range::Today,
            RangeArg::Last24h => Range::Last24Hours,
            RangeArg::Last7days => Range::Last7Days,
            RangeArg::Thismonth => Range::ThisMonth,
            RangeArg::Cycle => Range::BillingCycle,
        }
    }
}

#[derive(Args)]
pub struct ReportArgs {
    /// Preset range (ignored when --from and --to are both given)
    #[arg(long, value_enum, default_value_t = RangeArg::Today)]
    pub range: RangeArg,
    /// Start date, local time (YYYY-MM-DD)
    #[arg(long)]
    pub from: Option<String>,
    /// End date, local time, inclusive (YYYY-MM-DD)
    #[arg(long)]
    pub to: Option<String>,
    /// Restrict to one interface, e.g. en6
    #[arg(long)]
    pub iface: Option<String>,
    /// Emit JSON instead of a human-readable report
    #[arg(long)]
    pub json: bool,
}

/// Show the raw per-interface counters reported by the OS.
pub fn status() -> Result<()> {
    let config = Config::load()?;
    let unit = config.general.unit;

    let mut sampler = Sampler::new(RATE_WINDOW_MS);
    sampler.tick();
    let totals = sampler.raw_totals();
    if totals.is_empty() {
        println!("No network interfaces found.");
        return Ok(());
    }

    println!("Interfaces (counters since boot / last reset):");
    let mut all = Traffic::ZERO;
    for (name, traffic) in &totals {
        println!(
            "  {:<10} down {:>14}  up {:>14}",
            name,
            format_bytes(traffic.rx, unit),
            format_bytes(traffic.tx, unit)
        );
        all += *traffic;
    }
    println!(
        "  {:<10} down {:>14}  up {:>14}",
        "TOTAL",
        format_bytes(all.rx, unit),
        format_bytes(all.tx, unit)
    );
    Ok(())
}

/// Stream the current rate to stdout. Read-only: never touches the database.
pub fn sample(interval: u64) -> Result<()> {
    let config = Config::load()?;
    let unit = config.general.unit;
    let interval = interval.max(1);

    let mut sampler = Sampler::new(RATE_WINDOW_MS);
    sampler.tick();
    loop {
        sleep(Duration::from_secs(interval));
        let tick = sampler.tick();
        let stamp = Local::now().format("%H:%M:%S");
        println!(
            "[{stamp}] down {:>12}  up {:>12}",
            format_rate(tick.rate.rx_per_sec, unit),
            format_rate(tick.rate.tx_per_sec, unit)
        );
    }
}

/// Report stored usage for a range.
pub fn report(args: ReportArgs) -> Result<()> {
    let config = Config::load()?;
    let unit = config.general.unit;
    let store = Store::open(&Config::db_path()?)?;

    let range = match (args.from.as_deref(), args.to.as_deref()) {
        (Some(from), Some(to)) => Range::Custom {
            from: local_timestamp(parse_date(from)?)?,
            to: local_timestamp(parse_date(to)?.succ_opt().context("date out of range")?)?,
        },
        (None, None) => args.range.to_range(),
        _ => bail!("--from and --to must be used together"),
    };

    let (from, to) = range.resolve(Local::now(), &config.plan);
    let traffic = store.query_range(from, to, args.iface.as_deref())?;

    if args.json {
        #[derive(Serialize)]
        struct Report<'a> {
            from: i64,
            to: i64,
            range: &'a str,
            iface: Option<&'a str>,
            rx: u64,
            tx: u64,
            total: u64,
        }
        let payload = Report {
            from,
            to,
            range: range.label(),
            iface: args.iface.as_deref(),
            rx: traffic.rx,
            tx: traffic.tx,
            total: traffic.total(),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("NetMeter report");
        println!("  Range      {}", range.label());
        println!("  Window     {}  ->  {}", fmt_ts(from), fmt_ts(to));
        println!("  Interface  {}", args.iface.as_deref().unwrap_or("all"));
        println!("  Download   {}", format_bytes(traffic.rx, unit));
        println!("  Upload     {}", format_bytes(traffic.tx, unit));
        println!("  Total      {}", format_bytes(traffic.total(), unit));
    }
    Ok(())
}

/// Print where config and data live, plus the active config.
pub fn print_config() -> Result<()> {
    println!("Config   {}", Config::config_path()?.display());
    println!("Database {}", Config::db_path()?.display());
    println!();
    print!("{}", Config::load()?.to_toml()?);
    Ok(())
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("invalid date `{value}`, expected YYYY-MM-DD"))
}

fn local_timestamp(date: NaiveDate) -> Result<i64> {
    let naive = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    Ok(Local
        .from_local_datetime(&naive)
        .earliest()
        .context("ambiguous local time")?
        .timestamp())
}

fn fmt_ts(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}
