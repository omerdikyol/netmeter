#!/bin/sh
# Assemble the website into dist/site (ignored by git): the static pages plus the app's real panel UI,
# with panel-shim.js loaded first so the panel runs on made-up data.
set -eu
cd "$(dirname "$0")/.."
rm -rf dist/site
mkdir -p dist/site
cp site/index.html site/panel-shim.js dist/site/
cp docs/assets/icon.svg docs/screenshots/panel.png docs/screenshots/range.png docs/screenshots/settings.png dist/site/
sed 's|<head>|<head>\n<script src="panel-shim.js"></script>|' ui/index.html > dist/site/panel.html
grep -q 'panel-shim.js' dist/site/panel.html
