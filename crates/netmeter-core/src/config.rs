use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Which quantity the menu bar itself shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MenuBarMode {
    /// Just the activity glyph (default); the numbers live in the panel.
    Icon,
    /// Live transfer rate.
    Rate,
    /// Total transferred in the current billing cycle.
    Total,
}

/// Unit system for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitSystem {
    /// Decimal for volumes (matches carrier billing).
    Auto,
    /// Powers of 1024.
    Binary,
    /// Powers of 1000.
    Decimal,
}

/// How often the data plan resets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cycle {
    Weekly,
    Monthly,
}

/// Which appearance the panel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the system setting.
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub sample_interval_ms: u64,
    /// Start NetMeter when you log in, so recording has no gaps.
    pub launch_at_login: bool,
    pub menu_bar: MenuBarMode,
    pub unit: UnitSystem,
}

impl Default for General {
    fn default() -> Self {
        Self {
            sample_interval_ms: 1000,
            launch_at_login: false,
            // A quiet icon by default; the numbers live in the panel.
            menu_bar: MenuBarMode::Icon,
            unit: UnitSystem::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    /// Opacity of the panel background, 0.35 (very translucent) to 1.0.
    pub opacity: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            opacity: 0.72,
        }
    }
}

impl Appearance {
    /// Opacity, kept inside the range the panel can actually use.
    pub fn opacity(&self) -> f64 {
        if self.opacity.is_finite() {
            self.opacity.clamp(0.35, 1.0)
        } else {
            Appearance::default().opacity
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub enabled: bool,
    /// Data cap in bytes for the cycle.
    pub cap_bytes: u64,
    pub cycle: Cycle,
    /// Day of month the cycle resets (1-31). Clamped to the month length.
    pub reset_day: u32,
    /// Fractions of the cap at which to notify (0.8 = 80%).
    pub warn_at: Vec<f64>,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            enabled: false,
            cap_bytes: 0,
            cycle: Cycle::Monthly,
            reset_day: 1,
            warn_at: vec![0.8, 1.0],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
    pub plan: Plan,
}

impl Config {
    fn project_dirs() -> Result<ProjectDirs> {
        ProjectDirs::from("dev", "omerdikyol", "netmeter")
            .context("could not determine the user config directory")
    }

    pub fn config_dir() -> Result<PathBuf> {
        Ok(Self::project_dirs()?.config_dir().to_path_buf())
    }

    pub fn data_dir() -> Result<PathBuf> {
        Ok(Self::project_dirs()?.data_dir().to_path_buf())
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn db_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("usage.db"))
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("failed to serialize config")
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Load the user config, writing a commented default file on first run.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            let config = Self::default();
            config.save_to(&path).ok();
            return Ok(config);
        }
        Self::load_from(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, self.to_toml()?)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::config_path()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.general.sample_interval_ms, 1000);
        assert_eq!(c.general.menu_bar, MenuBarMode::Icon);
        assert!(!c.general.launch_at_login);
        assert_eq!(c.appearance.theme, Theme::System);
        assert!(!c.plan.enabled);
        assert_eq!(c.plan.warn_at, vec![0.8, 1.0]);
    }

    #[test]
    fn opacity_is_kept_usable() {
        let mut appearance = Appearance::default();
        assert!((appearance.opacity() - 0.72).abs() < 1e-9);
        appearance.opacity = 0.0;
        assert!((appearance.opacity() - 0.35).abs() < 1e-9);
        appearance.opacity = 5.0;
        assert!((appearance.opacity() - 1.0).abs() < 1e-9);
        appearance.opacity = f64::NAN;
        assert!((appearance.opacity() - 0.72).abs() < 1e-9);
    }

    #[test]
    fn appearance_round_trips() {
        let mut c = Config::default();
        c.appearance.theme = Theme::Dark;
        c.appearance.opacity = 0.5;
        c.general.launch_at_login = true;

        let back: Config = toml::from_str(&c.to_toml().unwrap()).unwrap();
        assert_eq!(back.appearance.theme, Theme::Dark);
        assert!((back.appearance.opacity - 0.5).abs() < 1e-9);
        assert!(back.general.launch_at_login);
    }

    #[test]
    fn toml_round_trip() {
        let mut c = Config::default();
        c.plan.enabled = true;
        c.plan.cap_bytes = 50_000_000_000;
        c.plan.reset_day = 15;

        let text = c.to_toml().unwrap();
        let back: Config = toml::from_str(&text).unwrap();

        assert!(back.plan.enabled);
        assert_eq!(back.plan.cap_bytes, 50_000_000_000);
        assert_eq!(back.plan.reset_day, 15);
    }

    #[test]
    fn partial_toml_falls_back_to_defaults() {
        let c: Config = toml::from_str("[general]\nsample_interval_ms = 500\n").unwrap();
        assert_eq!(c.general.sample_interval_ms, 500);
        assert_eq!(c.general.menu_bar, MenuBarMode::Icon);
    }

    #[test]
    fn config_from_an_older_build_still_loads() {
        // Keys and sections we have dropped must not stop the file parsing.
        let text = "[general]\nsample_interval_ms = 500\nlaunch_at_login = true\n\
                    remove_me = 1\n\n\
                    [tracking]\ninterfaces = [\"en0\"]\nfollow_default = false\n";
        let c: Config = toml::from_str(text).expect("an older config should still load");
        assert_eq!(c.general.sample_interval_ms, 500);
        assert!(c.general.launch_at_login);
        assert_eq!(c.general.menu_bar, MenuBarMode::Icon);
    }
}
