# Homebrew tap for Sable

1. Create a public repo named `homebrew-sable` under your GitHub user.
2. Copy `Casks/sable.rb` into that repo at `Casks/sable.rb`.
3. On each release, update `version` and `sha256` from `SHA256SUMS.txt` on the GitHub release.

Users install:

```bash
brew tap h3ma209/sable
brew install --cask sable
```

To submit to [homebrew-cask](https://github.com/Homebrew/homebrew-cask) later, open a PR there using the same cask definition once download volume justifies it.
