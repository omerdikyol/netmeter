//! The menu bar / tray application.

use crate::apps::AppMonitor;
use crate::icon_text::IconRenderer;
use crate::instance;
use crate::menu::{TrayMenu, ID_OPEN, ID_QUIT};
use crate::menu_icon;
use crate::notify;
use crate::panel::{Panel, PanelMessage, PanelState, SettingsRequest};
use anyhow::{Context, Result};
use netmeter_core::config::{Config, MenuBarMode};
use netmeter_core::format::{format_bytes, format_compact};
use netmeter_core::model::Rate;
use netmeter_core::stats::{self, Range};
use netmeter_core::Tracker;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tray_icon::menu::MenuEvent;
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Font size for the optional text title (`menu_bar = "rate" | "total"`).
const ICON_FONT_PX: f32 = 12.0;
/// Show the panel on launch at a fixed spot, for UI work and screenshots.
const PREVIEW_ENV: &str = "NETMETER_PREVIEW_PANEL";
/// Also pop the range sheet open on launch, for UI work.
const PREVIEW_SHEET_ENV: &str = "NETMETER_PREVIEW_SHEET";
/// Open the settings view on launch, for UI work.
const PREVIEW_SETTINGS_ENV: &str = "NETMETER_PREVIEW_SETTINGS";
/// Show the panel this many milliseconds after launch (instead of immediately),
/// so UI work can exercise the panel opening after the app has settled.
const PREVIEW_DELAY_ENV: &str = "NETMETER_PREVIEW_DELAY_MS";
/// Never dismiss the panel on focus loss; for staring at the UI while working.
const KEEP_OPEN_ENV: &str = "NETMETER_KEEP_OPEN";
/// Ignore a focus loss this soon after opening, so a click can finish landing.
const FOCUS_GRACE: Duration = Duration::from_millis(320);
/// How often to poll per-app usage.
const APP_SAMPLE_INTERVAL: Duration = Duration::from_secs(4);
/// Send the database-backed panel state every Nth sample; the live rate goes
/// out on every other one.
const STATE_EVERY: u32 = 3;

#[cfg(target_os = "macos")]
const ICON_COLOR: [u8; 4] = [0, 0, 0, 255];
#[cfg(not(target_os = "macos"))]
const ICON_COLOR: [u8; 4] = [255, 255, 255, 255];

/// Tray icon rect: `(x, y, width, height)` in physical pixels.
type Anchor = (f64, f64, f64, f64);

#[derive(Debug)]
enum UserEvent {
    TrayClicked(Option<Anchor>),
    Menu(MenuEvent),
    Ipc(String),
}

/// When to show the panel for UI work.
#[derive(Debug, Clone, Copy)]
enum AutoShow {
    Now,
    After(Duration),
}

struct App {
    tracker: Tracker,
    tray: TrayIcon,
    panel: Panel,
    apps: AppMonitor,
    renderer: Option<IconRenderer>,
    mode: MenuBarMode,
    range: Range,
    last_rate: Rate,
    last_live: HashMap<String, Rate>,
    last_icon_label: Option<String>,
    shown_at: Option<Instant>,
    ticks: u32,
    started: Instant,
    auto_show: Option<AutoShow>,
    keep_open: bool,
}

impl App {
    fn new(config: Config, tracker: Tracker, panel: Panel) -> Result<Self> {
        let mode = config.general.menu_bar;
        let auto_show = if std::env::var(PREVIEW_ENV).is_ok() {
            match std::env::var(PREVIEW_DELAY_ENV)
                .ok()
                .and_then(|ms| ms.parse::<u64>().ok())
            {
                Some(ms) => Some(AutoShow::After(Duration::from_millis(ms))),
                None => Some(AutoShow::Now),
            }
        } else {
            None
        };

        let mut builder = TrayIconBuilder::new()
            .with_tooltip("NetMeter")
            .with_icon(menu_icon::activity_icon())
            .with_menu(Box::new(TrayMenu::build().menu))
            // Left click opens the panel; right click shows the menu.
            .with_menu_on_left_click(false);
        #[cfg(target_os = "macos")]
        {
            builder = builder.with_icon_as_template(true);
        }
        let tray = builder.build().context("failed to create the tray icon")?;

        Ok(Self {
            tracker,
            tray,
            panel,
            apps: AppMonitor::start(APP_SAMPLE_INTERVAL),
            renderer: IconRenderer::new(),
            mode,
            range: Range::Today,
            last_rate: Rate::default(),
            last_live: HashMap::new(),
            last_icon_label: None,
            shown_at: None,
            ticks: 0,
            started: Instant::now(),
            auto_show,
            keep_open: std::env::var(KEEP_OPEN_ENV).is_ok(),
        })
    }

    /// Show the panel for UI work: at once, or after a delay so the opening
    /// path runs once the app has settled.
    fn maybe_auto_show(&mut self) {
        let Some(pending) = self.auto_show else {
            return;
        };
        match pending {
            AutoShow::Now => {
                self.auto_show = None;
                self.toggle_panel(None);
            }
            AutoShow::After(delay) if self.started.elapsed() >= delay => {
                self.auto_show = None;
                self.toggle_panel(None);
            }
            AutoShow::After(_) => {}
        }
    }

    fn toggle_panel(&mut self, anchor: Option<Anchor>) {
        if self.panel.is_visible() {
            self.panel.hide();
            return;
        }
        match anchor {
            Some((x, y, width, height)) => self.panel.show_under(x, y, width, height),
            None => self.panel.show_preview(),
        }
        self.shown_at = Some(Instant::now());
        self.push_state();
    }

    /// Ignore a focus loss this soon after opening, so the click that opened
    /// the panel can finish landing.
    fn on_focus_lost(&mut self) {
        if self.keep_open {
            return;
        }
        if let Some(at) = self.shown_at {
            if at.elapsed() < FOCUS_GRACE {
                return;
            }
        }
        self.panel.hide();
    }

    fn refresh(&mut self) {
        let tick = match self.tracker.tick() {
            Ok(tick) => tick,
            Err(err) => {
                eprintln!("netmeter: sampling failed: {err:#}");
                return;
            }
        };
        self.last_rate = tick.rate;
        self.last_live = tick.per_interface_rate;

        // Only the text modes need the cycle total, and it costs a query.
        if self.mode != MenuBarMode::Icon {
            match self.tracker.cycle_usage() {
                Ok(cycle) => self.update_title(tick.rate, cycle.total()),
                Err(err) => eprintln!("netmeter: {err:#}"),
            }
        }

        // The live rate is free to send every second; the rest of the panel
        // needs several queries, so it goes out less often.
        self.ticks = self.ticks.wrapping_add(1);
        if self.panel.is_visible() {
            if self.ticks % STATE_EVERY == 0 {
                self.push_state();
            } else {
                self.push_rate();
            }
        }
        if self.ticks % STATE_EVERY == 0 {
            self.check_alerts();
        }
    }

    /// Notify when the cycle crosses one of the configured thresholds.
    fn check_alerts(&mut self) {
        let plan = self.tracker.plan().clone();
        if !plan.enabled || plan.cap_bytes == 0 {
            return;
        }
        let used = match self.tracker.cycle_usage() {
            Ok(traffic) => traffic.total(),
            Err(err) => {
                eprintln!("netmeter: {err:#}");
                return;
            }
        };

        let cycle = self.tracker.cycle_window_start();
        let last = self.tracker.alert_state().unwrap_or(None);
        let Some(level) = stats::alert_due(used, plan.cap_bytes, &plan.warn_at, cycle, last) else {
            return;
        };

        let unit = self.tracker.config().general.unit;
        notify::send(
            if level >= 1.0 {
                "Data cap reached"
            } else {
                "Approaching your data cap"
            },
            &format!(
                "{} of {} used this cycle ({:.0}%).",
                format_bytes(used, unit),
                format_bytes(plan.cap_bytes, unit),
                stats::cap_percent(used, plan.cap_bytes)
            ),
        );

        if let Err(err) = self.tracker.record_alert(cycle, level) {
            eprintln!("netmeter: {err:#}");
        }
    }

    /// Persist settings from the panel and apply them without a restart.
    fn apply_settings(&mut self, request: SettingsRequest) {
        let mut config = self.tracker.config().clone();
        config.plan.enabled = request.cap_bytes > 0;
        config.plan.cap_bytes = request.cap_bytes;
        config.plan.reset_day = request.reset_day.clamp(1, 31);
        if !request.warn_at.is_empty() {
            config.plan.warn_at = request.warn_at;
        }
        if let Some(cycle) = parse_setting(&request.cycle) {
            config.plan.cycle = cycle;
        }
        if let Some(mode) = parse_setting(&request.menu_bar) {
            config.general.menu_bar = mode;
        }
        if let Some(unit) = parse_setting(&request.unit) {
            config.general.unit = unit;
        }

        if let Err(err) = config.save() {
            eprintln!("netmeter: could not save settings: {err:#}");
        }
        self.mode = config.general.menu_bar;
        self.tracker.set_config(config);
        // Force the title to redraw, including going back to the glyph.
        self.last_icon_label = None;
        if self.mode == MenuBarMode::Icon {
            apply_icon(&self.tray, menu_icon::activity_icon()).ok();
        }
        self.push_state();
    }

    fn push_rate(&mut self) {
        let rate = self.last_rate;
        if let Err(err) = self.panel.push_rate(rate.rx_per_sec, rate.tx_per_sec) {
            eprintln!("netmeter: {err:#}");
        }
    }

    /// Update the menu bar title for the optional text modes.
    fn update_title(&mut self, rate: Rate, cycle_total: u64) {
        let label = match self.mode {
            MenuBarMode::Icon => return,
            MenuBarMode::Total => format_compact(cycle_total as f64),
            MenuBarMode::Rate => {
                let down = rate.rx_per_sec >= rate.tx_per_sec;
                let arrow = match &self.renderer {
                    Some(renderer) => renderer.arrow(down),
                    None if down => 'v',
                    None => '^',
                };
                let value = if down {
                    rate.rx_per_sec
                } else {
                    rate.tx_per_sec
                };
                format!("{}{}", format_compact(value), arrow)
            }
        };

        if self.last_icon_label.as_deref() == Some(label.as_str()) {
            return;
        }
        let icon = match &self.renderer {
            Some(renderer) => {
                let (buffer, width, height) =
                    renderer.render(&label, ICON_FONT_PX, 1.0, ICON_COLOR);
                Icon::from_rgba(buffer, width, height)
                    .unwrap_or_else(|_| menu_icon::activity_icon())
            }
            None => menu_icon::activity_icon(),
        };
        if apply_icon(&self.tray, icon).is_ok() {
            self.last_icon_label = Some(label);
        }
    }

    fn push_state(&mut self) {
        if !self.panel.is_visible() {
            return;
        }
        let (_, apps) = self.apps.snapshot();
        let apps_error = self.apps.error();
        match PanelState::build(
            &self.tracker,
            self.range,
            self.last_rate,
            &self.last_live,
            apps,
            apps_error,
        ) {
            Ok(state) => {
                if let Err(err) = self.panel.push(&state) {
                    eprintln!("netmeter: {err:#}");
                }
            }
            Err(err) => eprintln!("netmeter: {err:#}"),
        }
    }

    /// Returns true when the app should quit.
    fn handle_ipc(&mut self, message: &str) -> bool {
        match serde_json::from_str::<PanelMessage>(message) {
            Ok(PanelMessage::Ready) => {
                self.push_state();
                false
            }
            Ok(PanelMessage::Range { value }) => {
                if let Some(range) = Range::from_key(&value) {
                    self.range = range;
                    self.push_state();
                }
                false
            }
            Ok(PanelMessage::Custom { from, to }) => {
                if to > from {
                    self.range = Range::Custom { from, to };
                    self.push_state();
                }
                false
            }
            Ok(PanelMessage::Save(request)) => {
                self.apply_settings(request);
                false
            }
            Ok(PanelMessage::Hide) => {
                self.panel.hide();
                false
            }
            Ok(PanelMessage::Quit) => true,
            Err(err) => {
                eprintln!("netmeter: ignoring panel message: {err}");
                false
            }
        }
    }

    fn shutdown(self) {
        let mut tracker = self.tracker;
        if let Err(err) = tracker.shutdown() {
            eprintln!("netmeter: failed to flush usage on exit: {err:#}");
        }
    }
}

/// Parse a config enum value the way the config file spells it, e.g. "monthly".
fn parse_setting<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(&format!("\"{value}\"")).ok()
}

/// Apply a new icon, preserving the macOS template flag.
///
/// The flag has to be re-applied for every new image, otherwise macOS draws our
/// black glyphs as-is and they vanish into a dark menu bar.
#[cfg(target_os = "macos")]
fn apply_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon_with_as_template(Some(icon), true)
}

#[cfg(not(target_os = "macos"))]
fn apply_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon(Some(icon))
}

pub fn run() -> Result<()> {
    let config = Config::load()?;
    let interval = Duration::from_millis(config.general.sample_interval_ms.clamp(200, 60_000));

    // Held for the whole process; the OS releases it on exit.
    let _instance = instance::acquire()?;

    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    let mut event_loop = builder.build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }

    let tray_proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            rect,
            ..
        } = event
        {
            let anchor = Some((
                rect.position.x,
                rect.position.y,
                rect.size.width as f64,
                rect.size.height as f64,
            ));
            let _ = tray_proxy.send_event(UserEvent::TrayClicked(anchor));
        }
    }));

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let mut ipc_proxy = Some(event_loop.create_proxy());
    let mut tracker_slot = Some(Tracker::open(config.clone())?);
    let mut app: Option<App> = None;
    let mut next_tick = Instant::now();
    let reveal = if std::env::var(PREVIEW_SHEET_ENV).is_ok() {
        Some("sheet")
    } else if std::env::var(PREVIEW_SETTINGS_ENV).is_ok() {
        Some("settings")
    } else {
        None
    };

    event_loop.run_return(|event, target, control_flow| {
        match event {
            // The tray icon and the panel window must be made once the loop runs.
            Event::NewEvents(StartCause::Init) => {
                if let Some(tracker) = tracker_slot.take() {
                    let built = ipc_proxy.take().map(|proxy| {
                        Panel::new(target, reveal, move |message| {
                            let _ = proxy.send_event(UserEvent::Ipc(message));
                        })
                        .and_then(|panel| App::new(config.clone(), tracker, panel))
                    });
                    match built {
                        Some(Ok(instance)) => app = Some(instance),
                        Some(Err(err)) => {
                            eprintln!("netmeter: {err:#}");
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                        None => {}
                    }
                    if let Some(instance) = app.as_mut() {
                        instance.maybe_auto_show();
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                ..
            } => {
                if let Some(instance) = app.as_mut() {
                    instance.on_focus_lost();
                }
            }
            Event::UserEvent(UserEvent::TrayClicked(anchor)) => {
                if let Some(instance) = app.as_mut() {
                    instance.toggle_panel(anchor);
                }
            }
            Event::UserEvent(UserEvent::Menu(menu_event)) => {
                let id = menu_event.id().0.as_str();
                if id == ID_QUIT {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                if id == ID_OPEN {
                    if let Some(instance) = app.as_mut() {
                        instance.toggle_panel(None);
                    }
                }
            }
            Event::UserEvent(UserEvent::Ipc(message)) => {
                if let Some(instance) = app.as_mut() {
                    if instance.handle_ipc(&message) {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
            }
            _ => {}
        }

        if let Some(instance) = app.as_mut() {
            instance.maybe_auto_show();
            let now = Instant::now();
            if now >= next_tick {
                instance.refresh();
                next_tick = Instant::now() + interval;
            }
        }

        *control_flow = ControlFlow::WaitUntil(next_tick);
    });

    if let Some(instance) = app {
        instance.shutdown();
    }
    Ok(())
}
