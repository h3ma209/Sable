# Published via https://github.com/h3ma209/homebrew-sable
cask "sable" do
  version "0.3.9"

  sha256 "c31a32515c1e22d01a1d183e0f4729ef96c462acb044d12ae6716b4cbb50c0cc"
  url "https://github.com/h3ma209/Sable/releases/download/v#{version}/Sable-#{version}-macos-arm64.dmg"

  name "Sable"
  desc "Open-source disk cleaner and space analyzer for macOS"
  homepage "https://github.com/h3ma209/Sable"
  depends_on arch: :arm64

  app "Sable.app"

  caveats <<~EOS
    Intel Macs: use the x64 .dmg from GitHub Releases (v0.3.10+) or build from source until the tap ships an Intel artifact.
  EOS

  zap trash: [
    "~/Library/Caches/dev.sable",
    "~/Library/Preferences/dev.sable.plist",
  ]
end
