//! The tray context menu.
//!
//! Left click opens the panel, so this is only what a right click shows: a way
//! back to the panel and a way out.

use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

pub const ID_OPEN: &str = "open";
pub const ID_QUIT: &str = "quit";

pub struct TrayMenu {
    pub menu: Menu,
}

impl TrayMenu {
    pub fn build() -> Self {
        let menu = Menu::new();
        let open = MenuItem::with_id(ID_OPEN, "Open NetMeter", true, None);
        let quit = MenuItem::with_id(ID_QUIT, "Quit NetMeter", true, None);

        let _ = menu.append(&open);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        Self { menu }
    }
}
