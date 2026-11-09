# Security Policy

Sable is a user-space macOS utility that reads and (with your confirmation) moves files to Trash. We take security reports seriously.

## Supported versions

| Version | Supported |
| --- | --- |
| Latest [GitHub Release](https://github.com/h3ma209/Sable/releases/latest) | Yes |
| Older releases | Best effort |

## Reporting a vulnerability

**Please do not file public GitHub issues for security bugs.**

1. Open a **private** [GitHub Security Advisory](https://github.com/h3ma209/Sable/security/advisories/new) on this repository, or
2. Email the maintainer via GitHub (`h3ma209`) with subject **Sable security**.

Include:

- Affected version or commit
- Steps to reproduce
- Impact (data loss, privilege escalation, remote code execution, etc.)
- Proof of concept if available

We aim to acknowledge reports within **72 hours** and share a fix timeline when we can reproduce the issue.

## What we consider in scope

- Arbitrary file deletion outside documented allow-lists
- Path traversal or sandbox escape in cleanup / trash routines
- Remote code execution in the app or installer
- Tampering with release artifacts or CI/CD (please include evidence)
- Privacy issues (unexpected network exfiltration of user files or paths)

## Out of scope

- Missing Apple notarization or Developer ID signing (known limitation; see README)
- macOS Gatekeeper warnings for unsigned downloads
- Social engineering against the user
- Denial of service from scanning very large directories on slow disks

## Architecture (for reviewers)

- Cleanup **only** trashes paths enumerated in `general_junk_candidates` / `dev_junk_candidates` (`RustUtilityApp/crates/engine/src/cleanup.rs`).
- Individual downloads are trashed only when the path is a **file under `~/Downloads`**.
- Deletes use **Finder Trash** via AppleScript, not `rm -rf`.
- The app does **not** require `sudo` for routine cache cleanup; some Tune-Up actions may invoke macOS tools that prompt for admin separately.

## Release integrity

Release assets ship with `SHA256SUMS.txt` on each tagged build. Verify downloads before opening:

```bash
shasum -a 256 --check SHA256SUMS.txt
```

Build from source if you do not trust pre-built binaries.
