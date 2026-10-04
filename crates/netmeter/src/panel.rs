//! The popover panel: a frameless, translucent window hosting the UI.
//!
//! The panel is a real window rather than a native menu, so we get the graph,
//! the range picker and the per-app rows. It renders in the system webview
//! (already present on the OS — no browser is bundled), is created hidden, and
//! is shown under the tray icon on click.

use crate::apps::AppUsage;
use anyhow::{Context, Result};
use chrono::Local;
use netmeter_core::config::{Cycle, MenuBarMode, Plan, Theme, UnitSystem};
use netmeter_core::model::{Rate, Traffic};
use netmeter_core::stats::{self, Range};
use netmeter_core::Tracker;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::HashMap;
use tao::dpi::LogicalSize;
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder};
use wry::{BackgroundThrottlingPolicy, WebView, WebViewBuilder};

const UI_HTML: &str = include_str!("../../../ui/index.html");

pub const PANEL_WIDTH: f64 = 340.0;
pub const PANEL_HEIGHT: f64 = 524.0;

/// Messages the panel sends back over IPC.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PanelMessage {
    Ready,
    Range {
        value: String,
    },
    /// An arbitrary `[from, to)` window in unix seconds.
    Custom {
        from: i64,
        to: i64,
    },
    /// The user saved the settings view.
    Save(SettingsRequest),
    /// Start at login, or stop doing that.
    #[serde(rename = "set-launch-at-login")]
    SetLaunchAtLogin {
        enabled: bool,
    },
    /// Forget all recorded usage.
    #[serde(rename = "clear-history")]
    ClearHistory,
    Hide,
    Quit,
}

/// What the settings view hands back. Empty or zero values mean "leave alone",
/// except `cap_bytes`, where zero means "no cap".
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRequest {
    pub cap_bytes: u64,
    pub cycle: String,
    pub reset_day: u32,
    pub warn_at: Vec<f64>,
    pub menu_bar: String,
    pub unit: String,
    pub theme: String,
    pub opacity: f64,
}

#[derive(Serialize)]
struct TrafficDto {
    rx: u64,
    tx: u64,
    total: u64,
}

impl From<Traffic> for TrafficDto {
    fn from(t: Traffic) -> Self {
        Self {
            rx: t.rx,
            tx: t.tx,
            total: t.total(),
        }
    }
}

#[derive(Serialize)]
struct RateDto {
    rx: f64,
    tx: f64,
}

#[derive(Serialize)]
struct PointDto {
    t: i64,
    rx: u64,
    tx: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanDto {
    enabled: bool,
    cap: u64,
    used: u64,
    percent: f64,
    cycle: &'static str,
    reset_day: u32,
    warn_at: Vec<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InterfaceDto {
    name: String,
    rx: u64,
    tx: u64,
    total: u64,
    rate_rx: f64,
    rate_tx: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelState {
    range: &'static str,
    range_label: &'static str,
    window: [i64; 2],
    /// Set when the user picked an explicit window.
    custom: Option<[i64; 2]>,
    rate: RateDto,
    series: Vec<PointDto>,
    /// Width of one series point, in seconds, so the UI can plot rates.
    bucket_secs: i64,
    totals: TrafficDto,
    today: TrafficDto,
    cycle: TrafficDto,
    plan: PlanDto,
    interfaces: Vec<InterfaceDto>,
    apps: Vec<AppUsage>,
    apps_supported: bool,
    apps_error: Option<String>,
    /// Current settings, so the settings view opens on the real values.
    unit: &'static str,
    menu_bar: &'static str,
    theme: &'static str,
    opacity: f64,
    /// When recording began, so a short total does not look like a bug.
    recording_since: Option<i64>,
    launch_at_login: bool,
    launch_at_login_available: bool,
    version: &'static str,
}

/// Bucket width that keeps the chart at a readable number of points.
fn bucket_for(range: Range) -> i64 {
    match range {
        Range::LastHour => stats::MINUTE,
        Range::Last6Hours | Range::Last12Hours => 5 * stats::MINUTE,
        Range::Today | Range::Last24Hours => 15 * stats::MINUTE,
        Range::Last7Days => 2 * stats::HOUR,
        Range::ThisMonth | Range::Last30Days | Range::BillingCycle => stats::DAY,
        Range::Custom { from, to } => ((to - from).max(1) / 100).max(stats::MINUTE),
    }
}

/// Query the series, narrowing the buckets while the window is mostly empty so
/// the graph has shape on a fresh install instead of one flat point.
fn series_for(tracker: &Tracker, range: Range) -> Result<(Vec<PointDto>, i64)> {
    let mut bucket = bucket_for(range);
    loop {
        let points: Vec<PointDto> = tracker
            .series(range, None, bucket)?
            .into_iter()
            .map(|(t, traffic)| PointDto {
                t,
                rx: traffic.rx,
                tx: traffic.tx,
            })
            .collect();
        if points.len() >= 8 || bucket <= stats::MINUTE {
            return Ok((points, bucket));
        }
        bucket = (bucket / 4).max(stats::MINUTE);
    }
}

impl PanelState {
    pub fn build(
        tracker: &Tracker,
        range: Range,
        rate: Rate,
        live: &HashMap<String, Rate>,
        apps: Vec<AppUsage>,
        apps_error: Option<String>,
    ) -> Result<Self> {
        let plan: &Plan = tracker.plan();
        let (from, to) = range.resolve(Local::now(), plan);

        let (mut series, bucket) = series_for(tracker, range)?;

        // Include the minute still in progress, scaled up to a whole bucket so
        // the last point does not dip simply because it is not finished yet.
        if let Some((start, traffic)) = tracker.current_bucket() {
            if start >= from && start < to {
                let elapsed = (stats::unix_now() - start).clamp(1, bucket) as f64;
                let scale = bucket as f64 / elapsed;
                let live = PointDto {
                    t: start,
                    rx: (traffic.rx as f64 * scale) as u64,
                    tx: (traffic.tx as f64 * scale) as u64,
                };
                match series.last_mut() {
                    Some(last) if last.t == start => *last = live,
                    _ => series.push(live),
                }
            }
        }

        let interfaces = tracker
            .per_interface(range)?
            .into_iter()
            .map(|(name, traffic)| {
                let live_rate = live.get(&name).copied().unwrap_or_default();
                InterfaceDto {
                    name,
                    rx: traffic.rx,
                    tx: traffic.tx,
                    total: traffic.total(),
                    rate_rx: live_rate.rx_per_sec,
                    rate_tx: live_rate.tx_per_sec,
                }
            })
            .collect();

        let cycle_traffic = tracker.cycle_usage()?;

        Ok(Self {
            range: range.key(),
            range_label: range.label(),
            window: [from, to],
            custom: match range {
                Range::Custom { from, to } => Some([from, to]),
                _ => None,
            },
            rate: RateDto {
                rx: rate.rx_per_sec,
                tx: rate.tx_per_sec,
            },
            series,
            bucket_secs: bucket,
            totals: tracker.report(range, None)?.into(),
            today: tracker.report(Range::Today, None)?.into(),
            cycle: cycle_traffic.into(),
            plan: PlanDto {
                enabled: plan.enabled && plan.cap_bytes > 0,
                cap: plan.cap_bytes,
                used: cycle_traffic.total(),
                percent: stats::cap_percent(cycle_traffic.total(), plan.cap_bytes),
                cycle: match plan.cycle {
                    Cycle::Weekly => "weekly",
                    Cycle::Monthly => "monthly",
                },
                reset_day: plan.reset_day,
                warn_at: plan.warn_at.clone(),
            },
            interfaces,
            apps,
            apps_supported: apps_error.is_none(),
            apps_error,
            unit: match tracker.config().general.unit {
                UnitSystem::Auto => "auto",
                UnitSystem::Binary => "binary",
                UnitSystem::Decimal => "decimal",
            },
            menu_bar: match tracker.config().general.menu_bar {
                MenuBarMode::Icon => "icon",
                MenuBarMode::Rate => "rate",
                MenuBarMode::Total => "total",
            },
            theme: match tracker.config().appearance.theme {
                Theme::System => "system",
                Theme::Light => "light",
                Theme::Dark => "dark",
            },
            opacity: tracker.config().appearance.opacity(),
            recording_since: tracker.recording_since(),
            launch_at_login: crate::login_item::is_enabled(),
            launch_at_login_available: crate::login_item::is_available(),
            version: env!("CARGO_PKG_VERSION"),
        })
    }
}

pub struct Panel {
    window: Window,
    webview: WebView,
    /// Whether the panel is currently drawn for a dark appearance.
    dark: Cell<bool>,
}

/// Bring the app to the front so the panel can become the key window.
///
/// `tao`'s `set_focus` only calls `makeKeyAndOrderFront`, which is not enough
/// for an accessory (menu bar only) app: without activating, the window never
/// becomes key, so it never *loses* focus — and losing focus is what dismisses
/// the panel when you click elsewhere.
#[cfg(target_os = "macos")]
fn activate_app() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let Some(marker) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(marker);
    app.activate();
}

#[cfg(not(target_os = "macos"))]
fn activate_app() {}

/// Blur behind the panel; the CSS only tints on top of it.
///
/// The material has to match the panel's appearance, or a light panel over a
/// dark blur looks washed out. Re-applying means clearing the old effect view
/// first, otherwise the two stack.
#[cfg(target_os = "macos")]
fn apply_material(window: &Window, dark: bool) {
    use window_vibrancy::{apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial};

    let material = if dark {
        NSVisualEffectMaterial::HudWindow
    } else {
        NSVisualEffectMaterial::Popover
    };
    let _ = clear_vibrancy(window);
    let _ = apply_vibrancy(window, material, None, Some(12.0));
}

#[cfg(not(target_os = "macos"))]
fn apply_material(_window: &Window, _dark: bool) {}

/// Where to put the panel, per platform.
///
/// On macOS this goes through AppKit rather than tao's `set_outer_position`,
/// which does not land where it is asked to. AppKit is also what can tell us
/// where the menu bar — and any notch — actually ends.
#[cfg(target_os = "macos")]
mod place {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWindow};
    use objc2_foundation::NSPoint;
    use tao::platform::macos::WindowExtMacOS;
    use tao::window::Window;

    /// Gap below the menu bar, and from the screen's side edges.
    const TOP_GAP: f64 = 5.0;
    const SIDE_GAP: f64 = 8.0;

    /// Put the window just under the menu bar, on the screen that owns the tray
    /// icon: centred on `anchor` (the tray icon's centre, in physical top-left
    /// coordinates) when given, else flush right on the main screen.
    pub fn under_menu_bar(window: &Window, anchor: Option<(f64, f64)>) {
        let Some(marker) = MainThreadMarker::new() else {
            return;
        };
        let screens = NSScreen::screens(marker);
        let Some(primary) = screens.iter().next() else {
            return;
        };

        let scale = window.scale_factor();
        let screen = match anchor {
            Some((anchor_x, anchor_y)) => {
                // AppKit's origin is the bottom left of the primary screen, so a
                // y measured downward from the top has to be flipped to find the
                // screen that contains it.
                let point = NSPoint::new(
                    anchor_x / scale,
                    primary.frame().size.height - anchor_y / scale,
                );
                screens
                    .iter()
                    .find(|screen| {
                        let frame = screen.frame();
                        point.x >= frame.origin.x
                            && point.x <= frame.origin.x + frame.size.width
                            && point.y >= frame.origin.y
                            && point.y <= frame.origin.y + frame.size.height
                    })
                    .unwrap_or(primary)
            }
            None => NSScreen::mainScreen(marker).unwrap_or(primary),
        };

        let visible = screen.visibleFrame();
        let size = window.outer_size().to_logical::<f64>(scale);

        let left = visible.origin.x + SIDE_GAP;
        let right = visible.origin.x + visible.size.width - size.width - SIDE_GAP;
        let wanted = match anchor {
            Some((anchor_x, _)) => anchor_x / scale - size.width / 2.0,
            None => right,
        };
        // AppKit's origin is bottom left, so the top edge is the largest y.
        let top = visible.origin.y + visible.size.height - TOP_GAP;

        let handle = window.ns_window();
        if handle.is_null() {
            return;
        }
        let ns_window: &NSWindow = unsafe { &*(handle as *const NSWindow) };
        ns_window.setFrameTopLeftPoint(NSPoint::new(wanted.clamp(left, right.max(left)), top));
    }
}

#[cfg(not(target_os = "macos"))]
mod place {
    use tao::dpi::PhysicalPosition;
    use tao::window::Window;

    pub fn under_menu_bar(window: &Window, _anchor: Option<(f64, f64)>) {
        let monitor = window
            .current_monitor()
            .or_else(|| window.primary_monitor());
        let Some(monitor) = monitor else {
            return;
        };
        let size = window.outer_size();
        let x =
            monitor.position().x as f64 + monitor.size().width as f64 - size.width as f64 - 16.0;
        window.set_outer_position(PhysicalPosition::new(x, 32.0));
    }
}

impl Panel {
    pub fn new<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        reveal: Option<&str>,
        theme: Theme,
        on_message: impl Fn(String) + 'static,
    ) -> Result<Self> {
        let window = WindowBuilder::new()
            .with_title("NetMeter")
            .with_inner_size(LogicalSize::new(PANEL_WIDTH, PANEL_HEIGHT))
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(true)
            .with_visible(false)
            .build(target)
            .context("failed to create the panel window")?;

        let dark = match theme {
            Theme::Dark => true,
            Theme::Light => false,
            // Resolve the system setting once, from the window's own appearance.
            Theme::System => !matches!(window.theme(), tao::window::Theme::Light),
        };
        apply_material(&window, dark);

        let mut builder = WebViewBuilder::new()
            .with_html(UI_HTML)
            .with_transparent(true)
            // Without this the webview is suspended the moment the app is not
            // frontmost, and the panel is drawn blank the next time it is shown.
            .with_background_throttling(BackgroundThrottlingPolicy::Disabled);
        if let Some(what) = reveal {
            // Runs before the page's own scripts; the page opens that view once
            // it has state to show. Only used by the preview mode.
            builder =
                builder.with_initialization_script(format!("window.__netmeterReveal = {what:?};"));
        }
        let webview = builder
            .with_ipc_handler(move |request| on_message(request.body().clone()))
            .build(&window)
            .context("failed to create the panel webview")?;

        Ok(Self {
            window,
            webview,
            dark: Cell::new(dark),
        })
    }

    /// Switch the blur behind the panel when the appearance setting changes.
    pub fn set_theme(&self, theme: Theme) {
        let dark = match theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::System => !matches!(self.window.theme(), tao::window::Theme::Light),
        };
        if self.dark.get() == dark {
            return;
        }
        self.dark.set(dark);
        apply_material(&self.window, dark);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// Show the panel anchored under the tray icon, centred on it.
    pub fn show_under(&self, x: f64, y: f64, width: f64, height: f64) {
        self.reveal(Some((x + width / 2.0, y + height / 2.0)));
    }

    /// Show the panel at the top right; used by the preview mode.
    pub fn show_preview(&self) {
        self.reveal(None);
    }

    /// Order the panel front and put it just under the menu bar.
    fn reveal(&self, anchor: Option<(f64, f64)>) {
        self.window.set_visible(true);
        place::under_menu_bar(&self.window, anchor);
        activate_app();
        self.window.set_focus();
    }

    pub fn push(&self, state: &PanelState) -> Result<()> {
        let json = serde_json::to_string(state)?;
        // Keep the payload from ever closing the script tag it is embedded in.
        let safe = json.replace('<', "\\u003c");
        self.webview.evaluate_script(&format!(
            "window.netmeter && window.netmeter.update({safe})"
        ))?;
        Ok(())
    }

    /// Push just the live rate, which needs no database work and so can be sent
    /// every tick while the heavier state is sent less often.
    pub fn push_rate(&self, rx_per_sec: f64, tx_per_sec: f64) -> Result<()> {
        let rate = RateDto {
            rx: rx_per_sec,
            tx: tx_per_sec,
        };
        let json = serde_json::to_string(&rate)?;
        self.webview
            .evaluate_script(&format!("window.netmeter && window.netmeter.rate({json})"))?;
        Ok(())
    }
}
