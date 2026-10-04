# Changelog

## 0.1.0

First public release. macOS only.

**The panel**

- Live download and upload rate, updated every second.
- A graph over the selected range, with 1H / 6H / 12H / 24H / 7D / 30D / Today /
  Cycle presets, an explicit From–To picker, and drag-across-the-graph to zoom.
  A Reset button returns to the last preset.
- Usage by app (since launch, from `nettop`) or by interface (totals for the
  range).
- Today's and this cycle's totals, plus progress against your cap.

**Data cap**

- Configurable size, reset cycle and reset day, all from the panel.
- Desktop notifications at the thresholds you choose, once per level per cycle.

**Menu bar**

- An activity glyph, the live rate, or the cycle total.

**Command line**

- `status`, `report`, `sample` and `config`. Reports are read-only: they never
  touch the stored history.

**Packaging**

- Universal macOS build (Apple silicon and Intel), Homebrew cask, and a
  generated app icon.

Releases are unsigned for now, so macOS warns on the first launch of a
downloaded copy — right-click and choose Open.
