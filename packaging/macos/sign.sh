#!/bin/sh
# Sign and notarize the app bundle, when the credentials are available.
#
# Usage: packaging/macos/sign.sh [path/to/NetMeter.app]
#
# Environment:
#   MACOS_CERTIFICATE           base64 of a "Developer ID Application" .p12
#   MACOS_CERTIFICATE_PASSWORD  the password used when exporting that .p12
#   APPLE_ID                    Apple account email, for notarization
#   APPLE_TEAM_ID               team id, for notarization
#   APPLE_APP_PASSWORD          an app-specific password, for notarization
#
# With MACOS_CERTIFICATE unset this prints what is missing and exits 0, so an
# unsigned release still builds and publishes.
set -eu

app=${1:-dist/NetMeter.app}
identity="Developer ID Application"

if [ ! -d "$app" ]; then
    echo "error: no app bundle at $app" >&2
    exit 1
fi

if [ -z "${MACOS_CERTIFICATE:-}" ]; then
    echo "--- not signing ---"
    echo "MACOS_CERTIFICATE is not set, so the app will be unsigned."
    echo "Downloaders will have to right-click > Open the first time."
    echo "To sign: create a 'Developer ID Application' certificate, export it as"
    echo ".p12, base64 it, and set MACOS_CERTIFICATE / MACOS_CERTIFICATE_PASSWORD"
    echo "plus APPLE_ID / APPLE_TEAM_ID / APPLE_APP_PASSWORD as repo secrets."
    exit 0
fi

echo "--- importing the certificate ---"
keychain=build.keychain
keychain_password=$(openssl rand -hex 16)
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
echo "$MACOS_CERTIFICATE" | base64 --decode > certificate.p12
security import certificate.p12 -k "$keychain" \
    -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')

echo "--- signing ---"
codesign --force --deep --options runtime --timestamp \
    --sign "$identity" "$app"
codesign --verify --strict --verbose=2 "$app"

if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ] || [ -z "${APPLE_APP_PASSWORD:-}" ]; then
    echo "--- not notarizing ---"
    echo "Apple credentials are incomplete; the app is signed but not notarized."
    exit 0
fi

echo "--- notarizing (this waits for Apple) ---"
ditto -c -k --keepParent "$app" notarize.zip
xcrun notarytool submit notarize.zip \
    --apple-id "$APPLE_ID" \
    --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" \
    --wait
xcrun stapler staple "$app"
xcrun stapler validate "$app"
echo "--- done: signed and notarized ---"
