cask "sable" do
  version "0.3.10"
  sha256 "b95d3fdccde287116fb55f71c8d0328d2138de81f8dc0c0fb132906d107b5c65"
  url "https://github.com/h3ma209/Sable/releases/download/v0.3.10/Sable-0.3.10-macos-arm64.dmg"

  name "Sable"
  desc "Open-source disk cleaner and space analyzer for macOS"
  homepage "https://github.com/h3ma209/Sable"
  depends_on arch: :arm64

  app "Sable.app"

  zap trash: [
    "~/Library/Caches/dev.sable",
    "~/Library/Preferences/dev.sable.plist",
  ]
end
