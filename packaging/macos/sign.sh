#!/bin/sh
# Sign, notarize and staple a target — a .app or a .dmg.
#
# Usage: packaging/macos/sign.sh [path]
#   default path: dist/NetMeter.app
#
# With no credentials this explains what is missing and exits 0, so an unsigned
# release still builds and publishes.
#
# Credentials for signing (always needed):
#   MACOS_CERTIFICATE           base64 of a "Developer ID Application" .p12
#   MACOS_CERTIFICATE_PASSWORD  the password used when exporting that .p12
#
# Credentials for notarization (one of these three, in this order of preference):
#   NOTARY_PROFILE  a keychain profile made by `notarytool store-credentials`.
#                   Most convenient locally, since nothing goes in the env.
#   NOTARY_KEY_P8   contents of an App Store Connect .p8 key   (best for CI: no
#   NOTARY_KEY_ID   its key id                                  personal password
#   NOTARY_ISSUER_ID  the issuer id                             and no 2FA)
# or
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD
#
# See SIGNING.md for how to obtain each of these.
set -eu

target=${1:-dist/NetMeter.app}
identity="Developer ID Application"

if [ ! -e "$target" ]; then
    echo "error: nothing to sign at $target" >&2
    exit 1
fi

key_file=""
cleanup() {
    rm -f certificate.p12 notarize.zip
    if [ -n "$key_file" ]; then rm -f "$key_file"; fi
}
trap cleanup EXIT

# Two ways to be signable: a .p12 in the environment (how CI does it, since a
# runner starts with nothing), or the certificate already sitting in this
# machine's keychain (how you do it locally, with nothing to export).
signed_ready=0
if [ -n "${MACOS_CERTIFICATE:-}" ]; then
    echo "--- importing the certificate ---"
    keychain=netmeter-signing.keychain
    keychain_password=$(openssl rand -hex 16)
    security create-keychain -p "$keychain_password" "$keychain"
    security set-keychain-settings -lut 3600 "$keychain"
    security unlock-keychain -p "$keychain_password" "$keychain"
    printf '%s' "$MACOS_CERTIFICATE" | base64 --decode > certificate.p12
    security import certificate.p12 -k "$keychain" -P "$MACOS_CERTIFICATE_PASSWORD" \
        -T /usr/bin/codesign -T /usr/bin/security
    security set-key-partition-list -S apple-tool:,apple:,codesign: \
        -s -k "$keychain_password" "$keychain" >/dev/null
    security list-keychains -d user -s "$keychain" $(security list-keychains -d user | sed 's/"//g')

    # Fail here, clearly, rather than with a confusing "no identity found" later.
    if ! security find-identity -v -p codesigning "$keychain" | grep -q "$identity"; then
        echo "error: no '$identity' identity after importing the certificate" >&2
        security find-identity -v -p codesigning "$keychain" >&2 || true
        exit 1
    fi
    signed_ready=1
elif security find-identity -v -p codesigning 2>/dev/null | grep -q "$identity"; then
    echo "--- signing with the Developer ID identity already in your keychain ---"
    signed_ready=1
fi

if [ "$signed_ready" -eq 0 ]; then
    echo "--- not signing ---"
    echo "No '$identity' identity available: MACOS_CERTIFICATE is not set, and"
    echo "this machine's keychain has no Developer ID certificate either."
    echo "Downloaders will have to right-click > Open the first time."
    echo "packaging/macos/SIGNING.md explains how to turn this on."
    exit 0
fi

echo "--- signing $target ---"
# No --deep: it is deprecated for signing as of macOS 13, and this bundle has no
# nested code for it to walk into anyway.
codesign --force --options runtime --timestamp --sign "$identity" "$target"
codesign --verify --strict --verbose=2 "$target"

# notarytool takes a .dmg directly; anything else has to be zipped.
case "$target" in
    *.dmg) submit="$target" ;;
    *)     submit="notarize.zip"; ditto -c -k --keepParent "$target" "$submit" ;;
esac

if [ -n "${NOTARY_KEY_P8:-}" ] && [ -z "${NOTARY_KEY:-}" ]; then
    key_file=$(mktemp -t authkey).p8
    printf '%s' "$NOTARY_KEY_P8" > "$key_file"
    NOTARY_KEY="$key_file"
fi

if [ -n "${NOTARY_PROFILE:-}" ]; then
    echo "--- notarizing with the '$NOTARY_PROFILE' keychain profile (this waits for Apple) ---"
    xcrun notarytool submit "$submit" --keychain-profile "$NOTARY_PROFILE" --wait
elif [ -n "${NOTARY_KEY:-}" ] && [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_ISSUER_ID:-}" ]; then
    echo "--- notarizing with an API key (this waits for Apple) ---"
    xcrun notarytool submit "$submit" \
        --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER_ID" \
        --wait
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    echo "--- notarizing with an Apple ID (this waits for Apple) ---"
    xcrun notarytool submit "$submit" \
        --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" \
        --wait
else
    echo "--- not notarizing ---"
    echo "No notary credentials, so $target is signed but not notarized."
    echo "Gatekeeper will still refuse it on another machine."
    exit 0
fi

xcrun stapler staple "$target"
xcrun stapler validate "$target"
echo "--- done: $target is signed and notarized ---"
