//! The tray context menu. Item text is updated in place as usage changes.

use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

pub const ID_QUIT: &str = "quit";

pub struct TrayMenu {
    /// Kept so later milestones can append interface and range entries.
    pub menu: Menu,
    rate: MenuItem,
    today: MenuItem,
    cycle: MenuItem,
}

impl TrayMenu {
    pub fn build() -> Self {
        let menu = Menu::new();

        let rate = MenuItem::with_id("rate", "NetMeter starting...", false, None);
        let today = MenuItem::with_id("today", "Today: 0 B", false, None);
        let cycle = MenuItem::with_id("cycle", "This month: 0 B", false, None);
        let quit = MenuItem::with_id(ID_QUIT, "Quit NetMeter", true, None);

        let _ = menu.append(&rate);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&today);
        let _ = menu.append(&cycle);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        Self {
            menu,
            rate,
            today,
            cycle,
        }
    }

    pub fn set_rate(&self, text: &str) {
        self.rate.set_text(text);
    }

    pub fn set_today(&self, text: &str) {
        self.today.set_text(text);
    }

    pub fn set_cycle(&self, text: &str) {
        self.cycle.set_text(text);
    }
}
