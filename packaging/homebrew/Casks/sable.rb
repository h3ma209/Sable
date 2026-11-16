cask "sable" do
  version "0.3.12"
  sha256 "5fd6b93c37c6d05835c27a8c426313f8c6ef97a897a1932da181220800b74f5d"
  url "https://github.com/h3ma209/Sable/releases/download/v0.3.12/Sable-0.3.12-macos-arm64.dmg"

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
