cask "netmeter" do
  version "0.1.0"
  sha256 "76a6c7ca9edb983d7dfd0b75eb24e3a26ac98fe88eed23555df53c197036ef8c"

  url "https://github.com/omerdikyol/netmeter/releases/download/v#{version}/NetMeter-macos-universal.dmg"
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
