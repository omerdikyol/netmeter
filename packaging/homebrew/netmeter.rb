cask "netmeter" do
  version "0.1.0"
  # Filled in from the release output: `cargo build --release` then
  # `shasum -a 256 dist/NetMeter-macos-universal.zip` (the release workflow
  # prints the same value).
  sha256 "PLACEHOLDER_SHA256"

  url "https://github.com/omerdikyol/netmeter/releases/download/v#{version}/NetMeter-macos-universal.zip"
  name "NetMeter"
  desc "Network usage monitor for the menu bar"
  homepage "https://github.com/omerdikyol/netmeter"

  # Universal build: Apple silicon and Intel.
  depends_on macos: ">= :ventura"

  app "NetMeter.app"

  caveats <<~EOS
    NetMeter lives in the menu bar. Left-click the icon for the panel.
    If the icon does not appear, a menu bar manager (Ice, Bartender) is
    probably hiding it: open that manager and move NetMeter to the
    always-visible section.
  EOS

  zap trash: [
    "~/Library/Application Support/dev.omerdikyol.netmeter",
  ]
end
