# Signing and notarizing NetMeter

Releases work unsigned, but macOS then refuses the first launch of a downloaded
copy and the README has to tell people to right-click and choose Open. Signing
and notarizing removes that, and needs an Apple Developer account.

Everything in this repository is already wired for it:
`packaging/macos/release.sh` signs and notarizes when the credentials are
present and skips it with an explanation when they are not, so nothing breaks
until you set them.

## What it costs and what you get

- **Apple Developer Program: $99/year.** There is no free route to a Developer
  ID certificate — this is the one part that cannot be worked around.
- With it: users double-click the app and it opens. Without it: right-click →
  Open, or `xattr -dr com.apple.quarantine`.
- Enrolling as an **individual** is fine and immediate. An **organization**
  account needs a D-U-N-S number and takes longer.

## One-time setup

### 1. Create a Developer ID certificate

1. Join the program at <https://developer.apple.com/programs>.
2. Open **Xcode → Settings → Accounts**, add your Apple ID, select the team, then
   **Manage Certificates → + → Developer ID Application**.
3. Open **Keychain Access**, find *Developer ID Application: Your Name*, right
   click → **Export**, choose `.p12`, and set a password when asked.

Check it landed:

```sh
security find-identity -v -p codesigning
```

### 2. Get notary credentials

Use an **App Store Connect API key**: it avoids putting your Apple ID password
in a repository secret and never trips two-factor auth.

1. <https://appstoreconnect.apple.com> → **Users and Access → Integrations →
   App Store Connect API → Team Keys → +**.
2. Give it the **Developer** role, create it, and note the **Key ID** and
   **Issuer ID**. Download the `.p8` file — Apple lets you download it once.

Alternatively use your Apple ID: create an **app-specific password** at
<https://account.apple.com> (Sign-In and Security → App-Specific Passwords), and
find your **Team ID** on the membership page.

### 3. Put them in the repository

```sh
# the certificate, base64'd in one blob
base64 -i Certificates.p12 | gh secret set MACOS_CERTIFICATE

gh secret set MACOS_CERTIFICATE_PASSWORD   # the .p12 export password

# the API key route
gh secret set NOTARY_KEY_P8    < AuthKey_ABCDE12345.p8
gh secret set NOTARY_KEY_ID    # e.g. ABCDE12345
gh secret set NOTARY_ISSUER_ID # a UUID

# or, instead, the Apple ID route
gh secret set APPLE_ID
gh secret set APPLE_TEAM_ID
gh secret set APPLE_APP_PASSWORD
```

The exact names are documented at the top of `packaging/macos/sign.sh`.

## Doing a signed release

Tag and push; the release workflow picks the secrets up and does the rest.

```sh
git tag -a v0.1.1 -m "NetMeter 0.1.1" && git push origin v0.1.1
```

To try it before trusting CI, run the same pipeline locally — it takes a few
minutes, most of it waiting on Apple:

```sh
export MACOS_CERTIFICATE=$(base64 -i Certificates.p12)
export MACOS_CERTIFICATE_PASSWORD=...
export NOTARY_KEY_P8=$(cat AuthKey_ABCDE12345.p8)
export NOTARY_KEY_ID=ABCDE12345
export NOTARY_ISSUER_ID=00000000-0000-0000-0000-000000000000

cargo build --release
packaging/macos/release.sh
```

## Checking it worked

`spctl` is the real test — it asks the same question Gatekeeper will:

```sh
spctl -a -vvv -t install dist/NetMeter.app   # want: accepted, source=Developer ID
xcrun stapler validate dist/NetMeter.app
xcrun stapler validate dist/NetMeter-macos-universal.dmg
codesign --verify --strict --verbose=2 dist/NetMeter.app
```

The most reliable end-to-end check is a machine that has never seen the app:
download the `.dmg` from the release page and open it.

## Once it is signed

- Delete the "Releases are unsigned for now" note from the README, and the
  quarantine line under it.
- The cask needs no workaround either; users just `brew install --cask netmeter`.
- Remember to update the cask's `sha256` after each release — the workflow prints
  the new hash.

## Notes

- Notarization cannot be done on an unsigned app, so sign first; the script does
  this in the right order.
- The ticket is **per artifact**: the app is notarized and stapled, then the disk
  image is notarized and stapled separately. That is why a release makes two
  submissions.
- NetMeter needs **no entitlements**. The hardened runtime is enough: it reads
  interface counters, and it spawns `nettop`, which a signed app is allowed to do.
- A signed, notarized build only proves the app has not been tampered with. It
  does not change what the app does — it still uploads nothing.
