//! Starting NetMeter at login, via a per-user LaunchAgent.
//!
//! A LaunchAgent is the right tool here: it needs no admin rights, it works for
//! an unsigned build, and undoing it is deleting one file. `SMAppService` would
//! be the modern choice, but it is fussier without a signed bundle.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const LABEL: &str = "dev.omerdikyol.netmeter";

/// The `.app` bundle we are running from, if we are in one.
///
/// Running the bare binary (`cargo run`) has nothing worth registering, so the
/// setting is unavailable there rather than silently broken.
pub fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        .map(Path::to_path_buf)
}

pub fn is_available() -> bool {
    bundle_path().is_some()
}

pub fn is_enabled() -> bool {
    plist_path().map(|path| path.exists()).unwrap_or(false)
}

/// Write the agent for wherever this copy of the app currently lives.
pub fn enable() -> Result<()> {
    let bundle = bundle_path().context("not running from an .app bundle")?;
    let path = plist_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, plist(&bundle))
        .with_context(|| format!("failed to write {}", path.display()))?;

    // `load -w` is deprecated but remains the most portable way to register an
    // agent from a plain process. The file on its own is honoured at next login.
    let _ = Command::new("/bin/launchctl")
        .args(["load", "-w"])
        .arg(&path)
        .output();
    Ok(())
}

pub fn disable() -> Result<()> {
    let path = plist_path()?;
    if path.exists() {
        let _ = Command::new("/bin/launchctl")
            .args(["unload", "-w"])
            .arg(&path)
            .output();
        std::fs::remove_file(&path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

/// Re-point an existing agent at wherever the app now lives, so moving the app
/// out of `dist/` and into `/Applications` does not silently break it.
pub fn refresh() {
    if is_enabled() {
        let _ = enable();
    }
}

fn plist_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn plist(bundle: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/open</string>
        <string>-a</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        bundle.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_points_at_the_bundle() {
        let xml = plist(Path::new("/Applications/NetMeter.app"));
        assert!(xml.contains("<string>/Applications/NetMeter.app</string>"));
        assert!(xml.contains("<key>RunAtLoad</key>"));
        assert!(xml.contains(LABEL));
    }

    #[test]
    fn a_bare_binary_has_no_bundle() {
        // Under `cargo test` the executable lives in target/debug, not a .app.
        assert!(bundle_path().is_none());
    }
}
