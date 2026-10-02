# NetMeter

A tiny, always-on network usage monitor for the macOS menu bar (and Windows /
Linux system tray). See your **current speed** at a glance and your **total
usage over any timeline** — handy when you are on a phone hotspot and need to
watch your data.

Built in Rust: a native tray plus the system webview for the panel. No Electron,
no bundled browser.

> Status: early development but usable. Left-click the menu bar icon for a panel
> with a live graph, any time range, per-app and per-interface totals, and cap
> progress. Everything is tested (see [Roadmap](#roadmap)); cap notifications and
> packaging are what is left.

## Why

Phone hotspots and mobile plans have data caps. NetMeter shows, live:

- the current download/upload rate,
- how much you have used today / this week / this month,
- progress against a data cap that resets on your billing day,
- optionally, a single interface (your iPhone tether) rather than everything.

## Design

- **No bundled browser.** The tray uses [`tray-icon`] + [`tao`], and the panel is
  drawn by the system webview (WebKit on macOS), so nothing like Chromium ships
  with the app. Sampling is just a counter read per second, so it is idle-cheap.
- **Cross-platform counters.** Per-interface bytes come from [`sysinfo`], which
  reads `getifaddrs` on macOS, `/proc/net/dev` on Linux and `GetIfTable2` on
  Windows.
- **Per-app usage on macOS.** Taken from the system `nettop`, sampled every few
  seconds and diffed, so per-app totals count only what happened while NetMeter
  was running. Windows and Linux would need their own mechanism.
- **UI-free core.** All logic lives in `netmeter-core` and is unit-tested; the
  panel, tray and CLI are thin layers on top.
- **Reset-safe.** Interface counters restart on reboot or link flap; NetMeter
  never reports a negative or inflated delta.
- **Local only.** History is a SQLite file on your machine. Nothing is uploaded.

## Install

Prebuilt binaries and a Homebrew formula are planned (see roadmap). For now,
build from source:

```sh
git clone https://github.com/omerdikyol/netmeter
cd netmeter
cargo build --release
```

On macOS, wrap the binary in a `.app` so the system treats it as a menu bar app
(no Dock icon) and launch it:

```sh
packaging/macos/bundle.sh
open dist/NetMeter.app
```

`./target/release/netmeter` also starts the tray app directly.

Requirements: Rust 1.80+. On Linux the tray needs an appindicator host:

```sh
sudo apt-get install libgtk-3-dev libayatana-appindicator3-dev
```

## Usage

Reporting works today from the CLI:

```sh
netmeter status                            # per-interface counters right now
netmeter report --range today              # usage today (default)
netmeter report --range cycle              # usage this billing cycle
netmeter report --from 2026-09-01 --to 2026-10-01
netmeter report --iface en6 --json         # one interface, machine-readable
netmeter sample --interval 1               # live rate in the terminal
netmeter config                            # show paths and current config
```

Running `netmeter` with no subcommand starts the menu bar app. The bar itself
stays quiet (an activity glyph by default); **left-click it** for the panel, or
right-click for a small menu.

## The panel

- **Live graph** of download (filled) and upload (dashed), with a hover readout.
- **Any time range**: quick chips (1H / 6H / 12H / 24H / 7D / 30D / Today /
  Cycle), a custom range with explicit From and To dates and times, and
  **drag across the chart** to zoom straight into a window. A **Reset** button
  appears whenever you are on a custom range and returns you to the last preset.
- **Totals** for today and the current cycle, plus cap progress and the reset day
  when a plan is configured.
- **Usage by Apps or Interfaces** — per-app totals since launch (macOS), or
  per-interface totals for the selected range.

### If the icon does not appear

On macOS, menu bar managers such as [Ice] or Bartender hide new items by
default: they are moved off-screen into a "hidden" section until you reveal
them. If NetMeter is running but you cannot see it, open that manager (click its
icon or its settings) and move NetMeter to the always-visible section.

## Configuration

Config lives in the platform config directory and is created on first run.
Run `netmeter config` to see the exact paths.

```toml
[general]
sample_interval_ms = 1000
launch_at_login = false
menu_bar = "icon"      # icon | rate | total
unit = "auto"          # auto | binary | decimal

[tracking]
interfaces = []        # empty = all interfaces
follow_default = true  # follow the interface carrying the default route

[plan]
enabled = false
cap_bytes = 50000000000
cycle = "monthly"      # weekly | monthly
reset_day = 15
warn_at = [0.8, 1.0]
```

## Roadmap

- [x] **M0–M1** — workspace, config, reset-safe sampler, SQLite store, reporting
      engine, CLI, tests, CI
- [x] **M2** — menu bar app and the webview panel
- [x] **M3** — per-interface and per-app views, quick and arbitrary ranges,
      drag-to-zoom
- [ ] **M4** — threshold notifications against the cap (the cap UI is done)
- [ ] **M5** — cross-platform releases and Homebrew formula

## Project layout

```
crates/netmeter-core   sampling, storage, stats, config (no UI; unit-tested)
crates/netmeter        tray app, panel host, per-app sampler and CLI
ui/index.html          the panel itself (HTML/CSS/JS, no build step)
assets/fonts           bundled font, used when the bar shows a number
packaging/macos        Info.plist and a script to assemble a .app bundle
```

## Third-party assets

- `assets/fonts/JetBrainsMono-Regular.ttf` — JetBrains Mono, licensed under the
  SIL Open Font License 1.1 (see `assets/fonts/OFL.txt`). Bundled only to render
  the menu bar text.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

[`tray-icon`]: https://github.com/tauri-apps/tray-icon
[`tao`]: https://github.com/tauri-apps/tao
[`sysinfo`]: https://github.com/GuillaumeGomez/sysinfo
[Ice]: https://github.com/jordanbaird/Ice
