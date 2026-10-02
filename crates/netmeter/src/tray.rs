//! The menu bar / tray application.

use crate::icon_text::IconRenderer;
use crate::instance;
use crate::menu::{TrayMenu, ID_QUIT};
use anyhow::{Context, Result};
use netmeter_core::config::{Config, Cycle, MenuBarMode, UnitSystem};
use netmeter_core::format::{format_bytes, format_compact, format_rate};
use netmeter_core::model::{Rate, Traffic};
use netmeter_core::sampler::Tick;
use netmeter_core::stats::Range;
use netmeter_core::Tracker;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tray_icon::menu::MenuEvent;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Font size, in logical pixels, used for the menu bar text.
const ICON_FONT_PX: f32 = 12.0;
/// Update the database-backed totals every Nth sample.
const TOTALS_EVERY: u32 = 3;

#[cfg(target_os = "macos")]
const ICON_COLOR: [u8; 4] = [0, 0, 0, 255];
#[cfg(not(target_os = "macos"))]
const ICON_COLOR: [u8; 4] = [255, 255, 255, 255];

#[derive(Debug)]
enum UserEvent {
    /// Wakes the loop when the tray icon is clicked; the payload is not needed yet.
    TrayClicked,
    Menu(MenuEvent),
}

struct App {
    tracker: Tracker,
    menu: TrayMenu,
    tray: TrayIcon,
    renderer: Option<IconRenderer>,
    unit: UnitSystem,
    mode: MenuBarMode,
    ticks: u32,
    last_icon_label: Option<String>,
}

impl App {
    fn new(config: Config, tracker: Tracker) -> Result<Self> {
        let unit = config.general.unit;
        let mode = config.general.menu_bar;
        let menu = TrayMenu::build();
        let renderer = IconRenderer::new();

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu.menu.clone()))
            .with_tooltip("NetMeter - network usage")
            .with_icon(fallback_icon());
        #[cfg(target_os = "macos")]
        {
            builder = builder.with_icon_as_template(true);
        }
        let tray = builder.build().context("failed to create the tray icon")?;

        let mut app = Self {
            tracker,
            menu,
            tray,
            renderer,
            unit,
            mode,
            ticks: 0,
            last_icon_label: None,
        };
        app.update_icon(Rate::default(), Traffic::ZERO);
        Ok(app)
    }

    fn cycle_label(&self) -> &'static str {
        match self.tracker.plan().cycle {
            Cycle::Monthly => "This month",
            Cycle::Weekly => "This week",
        }
    }

    fn refresh(&mut self) {
        match self.tracker.tick() {
            Ok(tick) => {
                if let Err(err) = self.apply(&tick) {
                    eprintln!("netmeter: {err:#}");
                }
            }
            Err(err) => eprintln!("netmeter: sampling failed: {err:#}"),
        }
    }

    fn apply(&mut self, tick: &Tick) -> Result<()> {
        let unit = self.unit;
        self.menu.set_rate(&format!(
            "\u{2193} {}    \u{2191} {}",
            format_rate(tick.rate.rx_per_sec, unit),
            format_rate(tick.rate.tx_per_sec, unit)
        ));

        let cycle = self.tracker.cycle_usage()?;
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % TOTALS_EVERY == 0 {
            let today = self.tracker.report(Range::Today, None)?;
            self.menu
                .set_today(&format!("Today: {}", format_bytes(today.total(), unit)));
            self.menu.set_cycle(&format!(
                "{}: {}",
                self.cycle_label(),
                format_bytes(cycle.total(), unit)
            ));
        }

        self.update_icon(tick.rate, cycle);
        Ok(())
    }

    fn update_icon(&mut self, rate: Rate, cycle: Traffic) {
        let label = match self.mode {
            MenuBarMode::Icon => return,
            MenuBarMode::Total => format_compact(cycle.total() as f64),
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
                Icon::from_rgba(buffer, width, height).unwrap_or_else(|_| fallback_icon())
            }
            None => fallback_icon(),
        };

        // `set_icon` on its own drops the macOS template flag, which leaves
        // black glyphs invisible on a dark menu bar. Re-apply it with the image.
        if apply_icon(&self.tray, icon).is_ok() {
            self.last_icon_label = Some(label);
        }
    }

    /// Returns true when the app should quit.
    fn handle_menu(&mut self, event: &MenuEvent) -> bool {
        event.id().0.as_str() == ID_QUIT
    }

    fn shutdown(self) {
        let mut tracker = self.tracker;
        if let Err(err) = tracker.shutdown() {
            eprintln!("netmeter: failed to flush usage on exit: {err:#}");
        }
    }
}

/// Apply a new icon, preserving the macOS template flag.
///
/// The template flag must be re-applied for every new image, otherwise macOS
/// draws our black glyphs as-is and they vanish into a dark menu bar.
#[cfg(target_os = "macos")]
fn apply_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon_with_as_template(Some(icon), true)
}

#[cfg(not(target_os = "macos"))]
fn apply_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon(Some(icon))
}

/// A plain bar-chart glyph, used before the first render or if text rendering
/// is unavailable.
fn fallback_icon() -> Icon {
    let (width, height) = (22u32, 22u32);
    let mut buffer = vec![0u8; (width * height * 4) as usize];
    let bars = [(4u32, 9u32), (9u32, 5u32), (14u32, 1u32)];
    for (x, top) in bars {
        for y in top..height - 2 {
            for offset in 0..4u32 {
                let px = x + offset;
                if px >= width {
                    continue;
                }
                let idx = ((y * width + px) * 4) as usize;
                buffer[idx..idx + 4].copy_from_slice(&ICON_COLOR);
            }
        }
    }
    Icon::from_rgba(buffer, width, height).expect("valid icon dimensions")
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

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |_event| {
        let _ = proxy.send_event(UserEvent::TrayClicked);
    }));
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    // The sampler is moved in here, then handed to `App` on the first event.
    let mut tracker_slot = Some(Tracker::open(config.clone())?);
    let mut app: Option<App> = None;
    let mut next_tick = Instant::now();

    event_loop.run_return(|event, _target, control_flow| {
        match event {
            // The tray icon must be created once the loop is running.
            Event::NewEvents(StartCause::Init) => {
                if let Some(tracker) = tracker_slot.take() {
                    match App::new(config.clone(), tracker) {
                        Ok(instance) => app = Some(instance),
                        Err(err) => {
                            eprintln!("netmeter: {err:#}");
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                    }
                }
            }
            Event::UserEvent(UserEvent::Menu(menu_event)) => {
                if let Some(instance) = app.as_mut() {
                    if instance.handle_menu(&menu_event) {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
            }
            _ => {}
        }

        if let Some(instance) = app.as_mut() {
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
