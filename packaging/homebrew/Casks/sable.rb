cask "sable" do
  version "0.3.11"
  sha256 "37ba58efd142b9c031c171011145a1e82e1dcc65b8391e77fd3878f1e1be46d8"
  url "https://github.com/h3ma209/Sable/releases/download/v0.3.11/Sable-0.3.11-macos-arm64.dmg"

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
