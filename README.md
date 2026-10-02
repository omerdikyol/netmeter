# NetMeter

A tiny, always-on network usage monitor for the macOS menu bar (and Windows /
Linux system tray). See your **current speed** at a glance and your **total
usage over any timeline** — handy when you are on a phone hotspot and need to
watch your data.

Built in Rust with a native tray — no Electron, no webview.

> Status: early development. The sampling, storage and reporting core plus a CLI
> are working and tested (see [Roadmap](#roadmap)). The menu bar UI is next.

## Why

Phone hotspots and mobile plans have data caps. NetMeter shows, live:

- the current download/upload rate,
- how much you have used today / this week / this month,
- progress against a data cap that resets on your billing day,
- optionally, a single interface (your iPhone tether) rather than everything.

## Design

- **Native, not a webview.** Uses [`tray-icon`] + [`tao`] rather than Tauri or
  Electron, so it stays small and light for an always-running menubar app.
- **Cross-platform counters.** Per-interface bytes come from [`sysinfo`], which
  reads `getifaddrs` on macOS, `/proc/net/dev` on Linux and `GetIfTable2` on
  Windows.
- **UI-free core.** All logic lives in `netmeter-core` and is unit-tested; the
  tray and CLI are thin layers on top.
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

Running `netmeter` with no subcommand will start the menu bar app.

## Configuration

Config lives in the platform config directory and is created on first run.
Run `netmeter config` to see the exact paths.

```toml
[general]
sample_interval_ms = 1000
launch_at_login = false
menu_bar = "rate"      # rate | total | icon
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
- [ ] **M2** — menu bar / tray app with live rate
- [ ] **M3** — per-interface menu and range presets
- [ ] **M4** — data cap, reset cycle and threshold notifications
- [ ] **M5** — cross-platform releases and Homebrew formula

## Project layout

```
crates/netmeter-core   sampling, storage, stats, config (no UI; unit-tested)
crates/netmeter        tray app + CLI
```

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

[`tray-icon`]: https://github.com/tauri-apps/tray-icon
[`tao`]: https://github.com/tauri-apps/tao
[`sysinfo`]: https://github.com/GuillaumeGomez/sysinfo
