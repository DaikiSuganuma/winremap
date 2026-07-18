# Security Policy

winremap is a keyboard remapper: a maliciously modified build could act as a
keylogger. Please only use binaries from the official distribution channel
and verify them as described below.

## Reporting a vulnerability

Please report vulnerabilities via
[GitHub Private Vulnerability Reporting](https://github.com/DaikiSuganuma/winremap/security/advisories/new).
Do not open a public issue for security problems.

## Supported versions

Only the latest release receives security fixes.

## What winremap does NOT do (by design)

- No logging or persistence of keystrokes (debug output is key-name level at
  most and off by default)
- No network access of any kind: no telemetry, no auto-update

These properties are enforced as repository policy ([AGENTS.md](AGENTS.md))
and checked in review.

## Official distribution and verification

Official binaries are published **only** on
[GitHub Releases](https://github.com/DaikiSuganuma/winremap/releases).
Binaries from any other site are unofficial.

Each release includes `SHA256SUMS` and a GitHub build-provenance
attestation. To verify a download:

```powershell
# 1. Checksum matches SHA256SUMS
(Get-FileHash .\winremap.exe -Algorithm SHA256).Hash.ToLower()
Get-Content .\SHA256SUMS

# 2. Build provenance: proves the exe was built by this repository's
#    GitHub Actions release workflow (requires GitHub CLI)
gh attestation verify .\winremap.exe --repo DaikiSuganuma/winremap
```

If either check fails, delete the file and download again from the official
Releases page.
