; Inno Setup script for the WinRemap installer (ADR 0027).
; Built in CI by release.yml:  iscc /DAppVersion=<x.y.z> installer\winremap.iss
; Requires target\release\winremap.exe (run `cargo build --release` first).

; AppVersion is injected from the release tag; the fallback marks local builds.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; Never change AppId: it is how upgrades find the existing installation.
AppId={{707AFB56-1975-4DF4-8371-B50DD23E3DA1}
AppName=WinRemap
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersion}
AppPublisher=Daiki Suganuma
AppPublisherURL=https://github.com/DaikiSuganuma/winremap
AppSupportURL=https://github.com/DaikiSuganuma/winremap/issues
AppUpdatesURL=https://github.com/DaikiSuganuma/winremap/releases
; Per-user install, no UAC: hooks are per-session and elevation buys nothing
; under UIPI (ADR 0027). {autopf} resolves to %LOCALAPPDATA%\Programs here.
PrivilegesRequired=lowest
DefaultDirName={autopf}\WinRemap
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; The single-instance mutex (hook.rs) doubles as the "please close WinRemap
; before installing/uninstalling" signal.
AppMutex=Local\winremap-single-instance
SetupIconFile=..\assets\kbd.ico
UninstallDisplayIcon={app}\winremap.exe
LicenseFile=..\LICENSE
OutputDir=output
OutputBaseFilename=winremap-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[CustomMessages]
english.StartupTask=Start WinRemap automatically when you sign in
japanese.StartupTask=サインイン時に WinRemap を自動的に起動する
english.LaunchApp=Launch WinRemap now
japanese.LaunchApp=WinRemap を今すぐ起動する

[Tasks]
Name: "startup"; Description: "{cm:StartupTask}"

[Files]
Source: "..\target\release\winremap.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.ja.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\examples\*.toml"; DestDir: "{app}\examples"; Flags: ignoreversion
; Seed a starter config only when the user has none. User data: never
; overwritten on upgrade, never removed on uninstall.
Source: "..\examples\minimal.toml"; DestDir: "{userappdata}\winremap"; DestName: "config.toml"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{autoprograms}\WinRemap"; Filename: "{app}\winremap.exe"
; runminimized keeps the v0.1 console window out of the way at sign-in.
Name: "{userstartup}\WinRemap"; Filename: "{app}\winremap.exe"; Tasks: startup; Flags: runminimized

[Run]
Filename: "{app}\winremap.exe"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent
