# NetMeter

A small, quiet network usage monitor for the macOS menu bar. Left-click the icon
for a panel with a live graph, the total over any time range you like, and how
much of your data plan you have burnt through.

Built for the phone-hotspot case: tethering on a capped mobile plan, wanting to
know what you have used before the carrier tells you.

![platform: macOS](https://img.shields.io/badge/platform-macOS-blue)
![license: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green)
[![CI](https://github.com/omerdikyol/netmeter/actions/workflows/ci.yml/badge.svg)](https://github.com/omerdikyol/netmeter/actions/workflows/ci.yml)

![The NetMeter panel](docs/screenshots/panel.png)

## What it does

- **Live rate in the panel** — download and upload, updated every second.
- **Any time range** — quick chips (1H / 6H / 12H / 24H / 7D / 30D / Today /
  Cycle), an explicit From–To picker, or **drag across the graph** to zoom in on
  a window. A **Reset** button appears whenever you are zoomed and takes you back
  to the last preset.
- **Usage by app** — the busiest processes since launch, read from the system
  `nettop`, or switch to per-interface totals for the selected range.
- **A data cap that means something** — set your plan's size and reset day and the
  panel shows progress against it, with desktop notifications as you cross the
  thresholds you chose.

## Install

Homebrew, from a tap:

```sh
brew tap omerdikyol/netmeter
brew install --cask netmeter
```

Or download `NetMeter-macos-universal.zip` from
[Releases](https://github.com/omerdikyol/netmeter/releases), unzip, and drag
`NetMeter.app` to Applications.

> **Releases are unsigned for now.** macOS will refuse the first launch of a
> downloaded copy. Right-click the app and choose **Open**, or run
> `xattr -dr com.apple.quarantine /Applications/NetMeter.app`. Signing and
> notarization are wired up in the release workflow, but need an Apple Developer
> account to switch on.

From source:

```sh
git clone https://github.com/omerdikyol/netmeter
cd netmeter
cargo build --release
packaging/macos/bundle.sh
open dist/NetMeter.app
```

Requires Rust 1.80 or newer.

## Using it

NetMeter sits in the menu bar as a small activity glyph, so it stays out of the
way. **Left-click** it for the panel; **right-click** for a short menu.

The panel has four parts: the live rate at the top, the graph with its range
control, today's and this cycle's totals, and the usage list. The gear in the
footer opens settings, and **Quit** is next to it.

### If the icon does not appear

Menu bar managers such as [Ice](https://github.com/jordanbaird/Ice) or Bartender
hide new items by default — they are moved off-screen until you reveal them. If
NetMeter is running but you cannot see it, open that manager and move NetMeter to
the always-visible section.

## Screenshots

Choosing any time range, including an explicit From–To window:

![The range picker](docs/screenshots/range.png)

Settings, for the cap and its notifications:

![Settings](docs/screenshots/settings.png)

## Configuration

Settings live in the panel, but they are just a TOML file, created on first run.
`netmeter config` prints the paths and the current contents.

```toml
[general]
sample_interval_ms = 1000
menu_bar = "icon"      # icon | rate | total
unit = "auto"          # auto | binary | decimal

[plan]
enabled = true
cap_bytes = 50000000000
cycle = "monthly"      # weekly | monthly
reset_day = 15
warn_at = [0.8, 1.0]
```

History is a SQLite file next to the config, pruned to 90 days.

## The command line

The same engine is available without the tray:

```sh
netmeter status                             # per-interface counters right now
netmeter report --range today               # usage today
netmeter report --range cycle               # usage this billing cycle
netmeter report --from 2026-09-01 --to 2026-10-01
netmeter report --iface en6 --json          # one interface, machine readable
netmeter sample --interval 1                # live rate in the terminal
```

`report` and `sample` are read-only: they never touch the stored history.

## How it works

- **No bundled browser.** The tray is native ([`tray-icon`] + [`tao`]); the panel
  is drawn by the system webview, so nothing like Chromium ships with the app.
- **Counters, not guesses.** Per-interface bytes come from [`sysinfo`]
  (`getifaddrs` on macOS). Interface counters restart on reboot or link flap, so a
  value that goes backwards is treated as a reset — NetMeter never reports a
  negative or inflated delta.
- **Per-app usage** is `nettop` sampled every few seconds and diffed, so the
  numbers count only what happened while NetMeter was running.
- **Local only.** Everything is a file on your machine. Nothing is uploaded, and
  there are no accounts.
- **Small.** One binary plus the system webview; no background service.

## macOS only, for now

The panel renders through `wry`, which needs a GTK event loop on Linux that this
app does not yet set up, and Windows has never been exercised. Rather than claim
three platforms and ship one, this release says macOS and means it.
`crates/netmeter-core` — sampling, storage, statistics, configuration — is
platform-neutral and would be the base for other front ends.

## Development

The panel can be shown without clicking the tray icon, which is handy while
working on the UI:

```sh
NETMETER_PREVIEW_PANEL=1 cargo run                 # open the panel on launch
NETMETER_PREVIEW_PANEL=1 NETMETER_PREVIEW_DELAY_MS=4000 cargo run
                                                   # ...a few seconds later
NETMETER_PREVIEW_SHEET=1 cargo run                 # with the range picker open
NETMETER_PREVIEW_SETTINGS=1 cargo run              # on the settings screen
NETMETER_KEEP_OPEN=1 cargo run                     # never dismiss on focus loss
```

```
crates/netmeter-core   sampling, storage, stats, config (no UI; unit tested)
crates/netmeter        tray app, panel host, per-app sampler, CLI
ui/index.html          the panel itself (HTML/CSS/JS, no build step)
packaging/macos        Info.plist, bundle and signing scripts
packaging/homebrew     the cask
```

## Roadmap

- [x] Sampling, storage and reporting core, with a CLI
- [x] Menu bar app and the panel
- [x] Per-interface and per-app views, any time range
- [x] Data cap, reset cycle and threshold notifications
- [ ] Signed and notarized releases
- [ ] Linux and Windows front ends

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

The bundled JetBrains Mono font (used only when the menu bar shows a number) is
under the SIL Open Font License 1.1 — see `assets/fonts/OFL.txt`.

[`tray-icon`]: https://github.com/tauri-apps/tray-icon
[`tao`]: https://github.com/tauri-apps/tao
[`sysinfo`]: https://github.com/GuillaumeGomez/sysinfo
