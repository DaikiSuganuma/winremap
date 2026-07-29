# Security Policy

WinRemap is a keyboard remapper: a maliciously modified build could act as a
keylogger. Please only use binaries from an official distribution channel
and verify them as described below.

## Reporting a vulnerability

Please report vulnerabilities via
[GitHub Private Vulnerability Reporting](https://github.com/DaikiSuganuma/winremap/security/advisories/new).
Do not open a public issue for security problems.

## Supported versions

Only the latest release receives security fixes.

## What WinRemap does NOT do (by design)

- No logging or persistence of keystrokes (debug output is key-name level at
  most and off by default)
- No network access of any kind: no telemetry, no auto-update

These properties are enforced as repository policy ([AGENTS.md](AGENTS.md))
and checked in review.

## Official distribution and verification

WinRemap is published through **two** channels, both of them ours:

| Channel | Who signs it | How you verify it |
|---|---|---|
| [Microsoft Store](https://apps.microsoft.com/detail/9N6TQDXRX5WV) | Microsoft, on submission | The Store page itself: publisher **SUGANUMA Daiki**, product ID **9N6TQDXRX5WV** |
| [GitHub Releases](https://github.com/DaikiSuganuma/winremap/releases) | Unsigned | `SHA256SUMS` and a build-provenance attestation (below) |

Binaries from any other site are unofficial.

The Store package and the GitHub binaries are built from the same source at
the same tag. Choosing between them is a matter of how you want to install
and update, not of trust — see the
[install guide](https://daikisuganuma.github.io/winremap/install.html) for
the practical differences, including where each one keeps your config file.

> The Store listing goes live when certification completes for v0.6.0; until
> then the link above will not resolve.

### Verifying a GitHub Releases download

Each release includes `SHA256SUMS` and a GitHub build-provenance
attestation covering both the portable `winremap.exe` and the installer
`winremap-setup.exe`. To verify a download (swap in `winremap-setup.exe`
to check the installer):

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
