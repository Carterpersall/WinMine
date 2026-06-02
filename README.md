# WinMine

A Rust port of the classic Windows Minesweeper (WinMine). The goal is to faithfully recreate the original game's behavior and appearance, while fixing some bugs and modernizing the codebase.

## Build

This is Windows-only and uses Win32 APIs. Make sure you have Rust with the MSVC toolchain installed.

```bash
cargo build
```

For an optimized build:

```bash
cargo build --release
```

## Run

```bash
cargo run
```

Or launch the built binary directly:

- Debug: `target\debug\winmine.exe`
- Release: `target\release\winmine.exe`

## What is included

- Win32 GUI built with `winsafe` and `windows-sys`
- Registry-backed settings and best times under `HKCU\Software\Microsoft\winmine`
- DPI-aware window scaling

## Differences from the original WinMine

This project aims to be faithful, but there are a few deliberate changes and fixes:

- Window size has been tweaked, as the original is cut off on the bottom and right on modern systems.
- Preferences are saved on exit; the original only wrote settings in narrower cases.
- The Custom dialog only starts a new custom game if settings were actually saved; cancel leaves the current game alone.
- Invalid or empty Custom dialog input now keeps the dialog open instead of defaulting to the minimum.
- XYZZY mouse tracking only runs during an active game to avoid false positives.
- Saved window position is not clamped to 0..1024
- Legacy pre-registry ini migration is removed.
- Sound can always be toggled on or off by pressing F4, instead of only toggling when sound was already enabled. (On <-> Off instead of Off <- On <-> Muted)
- Window sizing assumes a single-row menu bar.
- Help on Help is served from the bundled `winmine.chm` instead of `NTHelp.chm`.
- `winmine.chm` is included into the executable and extracted to `%TEMP%\winmine.chm` as needed, instead of being a separate file in the installation directory.

## Notes

This is a reimplementation, binary-compatibility is not guaranteed. If you spot a behavior mismatch, file an issue with steps to reproduce.
