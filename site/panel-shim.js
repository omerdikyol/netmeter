// Stands in for the Rust side of the panel on the website. The page build copies
// ui/index.html to panel.html and loads this script first, so the real panel UI
// runs here against made-up traffic. Nothing is measured; no app names, interface
// names or totals belong to anyone.
(function () {
  var GB = 1e9, MIN = 60, HOUR = 3600, DAY = 86400;
  var LABELS = {
    "1h": "Last hour", "6h": "Last 6 hours", "12h": "Last 12 hours", today: "Today",
    "24h": "Last 24 hours", "7d": "Last 7 days", "30d": "Last 30 days",
    month: "This month", cycle: "Billing cycle", custom: "Custom range"
  };
  var APPS = [["Safari", 0.43], ["Xcode", 0.22], ["Slack", 0.09], ["Dropbox", 0.07], ["Mail", 0.02], ["Music", 0.01]];
  var settings = { unit: "auto", menuBar: "icon", theme: "dark", opacity: 1, launchAtLogin: true,
    plan: { enabled: true, cap: 50 * GB, cycle: "monthly", resetDay: 1, warnAt: [0.8, 1.0] } };
  var current = { range: "today", custom: null };

  function startOf(kind, now) {
    var d = new Date(now * 1000);
    if (kind === "day") d.setHours(0, 0, 0, 0);
    else { d.setDate(1); d.setHours(0, 0, 0, 0); }
    return Math.floor(d.getTime() / 1000);
  }
  function windowFor(range, now) {
    var back = { "1h": HOUR, "6h": 6 * HOUR, "12h": 12 * HOUR, "24h": DAY, "7d": 7 * DAY, "30d": 30 * DAY }[range];
    if (back) return [now - back, now];
    if (range === "today") return [startOf("day", now), now];
    if (range === "month" || range === "cycle") return [startOf("month", now), now];
    return current.custom;
  }
  // Same bucket widths as panel.rs, so the chart has the app's own density.
  function bucketFor(range, w) {
    return { "1h": MIN, "6h": 5 * MIN, "12h": 5 * MIN, today: 15 * MIN, "24h": 15 * MIN,
      "7d": 2 * HOUR, "30d": DAY, month: DAY, cycle: DAY }[range] || Math.max(MIN, Math.floor((w[1] - w[0]) / 100));
  }
  // Bytes per second at a moment: quiet nights, a busy afternoon, and a little noise.
  function rateAt(t) {
    var h = new Date(t * 1000).getHours() + new Date(t * 1000).getMinutes() / 60;
    var daily = 0.15 + 0.85 * Math.max(0, Math.sin(((h - 7) / 16) * Math.PI));
    var noise = 0.75 + 0.5 * Math.abs(Math.sin(t / 997) * Math.cos(t / 3301));
    return 95000 * daily * noise;
  }
  function traffic(from, to) {
    var rx = 0, step = Math.max(60, Math.floor((to - from) / 400));
    for (var t = from; t < to; t += step) rx += rateAt(t) * Math.min(step, to - t);
    rx = Math.round(rx);
    var tx = Math.round(rx * 0.14);
    return { rx: rx, tx: tx, total: rx + tx };
  }
  function state() {
    var now = Math.floor(Date.now() / 1000);
    var w = windowFor(current.range, now);
    var bucket = bucketFor(current.range, w);
    var series = [];
    for (var t = w[0]; t < w[1]; t += bucket) {
      var seg = traffic(t, Math.min(t + bucket, w[1]));
      series.push({ t: t, rx: seg.rx, tx: seg.tx });
    }
    var totals = traffic(w[0], w[1]);
    var cycle = traffic(startOf("month", now), now);
    var plan = settings.plan;
    return {
      range: current.range, rangeLabel: LABELS[current.range], window: w, custom: current.custom,
      rate: rate(), series: series, bucketSecs: bucket, totals: totals,
      today: traffic(startOf("day", now), now), cycle: cycle,
      plan: { enabled: plan.enabled && plan.cap > 0, cap: plan.cap, used: cycle.total,
        percent: plan.cap ? (cycle.total / plan.cap) * 100 : 0,
        cycle: plan.cycle, resetDay: plan.resetDay, warnAt: plan.warnAt },
      interfaces: [
        { name: "en0 · Wi-Fi", rx: Math.round(totals.rx * 0.82), tx: Math.round(totals.tx * 0.8),
          total: Math.round(totals.total * 0.82), rateRx: 61000, rateTx: 9000 },
        { name: "en6 · iPhone USB", rx: Math.round(totals.rx * 0.18), tx: Math.round(totals.tx * 0.2),
          total: Math.round(totals.total * 0.18), rateRx: 12000, rateTx: 2000 }
      ],
      apps: APPS.map(function (a) {
        var rx = Math.round(totals.rx * a[1]), tx = Math.round(totals.tx * a[1]);
        return { name: a[0], rx: rx, tx: tx, total: rx + tx, rateRx: 60000 * a[1], rateTx: 9000 * a[1] };
      }),
      appsSupported: true, appsError: null,
      unit: settings.unit, menuBar: settings.menuBar, theme: settings.theme, opacity: settings.opacity,
      recordingSince: startOf("month", now) - 20 * DAY,
      launchAtLogin: settings.launchAtLogin, launchAtLoginAvailable: true, version: "0.1.1"
    };
  }
  function rate() {
    var base = rateAt(Date.now() / 1000) * (0.6 + Math.random() * 0.9);
    return { rx: base, tx: base * (0.1 + Math.random() * 0.1) };
  }
  function push() { if (window.netmeter) window.netmeter.update(state()); }

  window.ipc = {
    postMessage: function (raw) {
      var m = JSON.parse(raw);
      if (m.type === "range") { current = { range: m.value, custom: null }; }
      else if (m.type === "custom") { current = { range: "custom", custom: [m.from, m.to] }; }
      else if (m.type === "save") {
        settings.unit = m.unit; settings.menuBar = m.menuBar; settings.theme = m.theme; settings.opacity = m.opacity;
        settings.plan = { enabled: m.capBytes > 0, cap: m.capBytes, cycle: m.cycle, resetDay: m.resetDay, warnAt: m.warnAt };
      } else if (m.type === "set-launch-at-login") { settings.launchAtLogin = m.enabled; }
      else if (m.type !== "ready") { return; } // hide, quit, clear-history: nothing to do on a web page
      setTimeout(push, 0);
    }
  };
  setInterval(function () { if (window.netmeter) window.netmeter.rate(rate()); }, 1000);
  setInterval(push, 30000);
})();
