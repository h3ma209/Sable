# Apple code signing & notarization (optional)

Releases work unsigned (ad-hoc). For Gatekeeper-friendly downloads, configure GitHub Actions **secrets**:

| Secret | Purpose |
| --- | --- |
| `APPLE_CERTIFICATE_BASE64` | `.p12` export, base64 |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `NOTARY_APPLE_ID` | Apple ID email |
| `NOTARY_APP_PASSWORD` | App-specific password |
| `NOTARY_TEAM_ID` | 10-char Team ID |

Optional local keychain profile instead of API creds:

| Secret | Purpose |
| --- | --- |
| `NOTARIZE_PROFILE` | Keychain profile name for `notarytool` |

When `APPLE_SIGNING_IDENTITY` and notary credentials are set, `scripts/package-macos.sh` notarizes the DMG and staples the app.

To enable certificate import on CI, add this step before `make package` in `.github/workflows/release.yml`:

```yaml
- uses: apple-actions/import-codesign-certs@v3
  with:
    p12-file-base64: ${{ secrets.APPLE_CERTIFICATE_BASE64 }}
    p12-password: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
```
