# PrismDisk

A fast, modern disk space analyzer for Windows. Like WizTree, it reads the NTFS **Master File Table directly**, so a full drive with millions of files is mapped in seconds (≈2.3 M files in ~4 s on a 1.8 TB SSD). It ships with a fully custom setup wizard.

100% native Rust: the UI is drawn with [egui](https://github.com/emilk/egui) on the GPU, so there is no browser engine, no WebView2 and no runtime to install. One process, a ~5 MB exe, ~100 MB of memory idle (a full 2.3 M-file scan of C: sits at ~350 MB, versus ~680 MB across 7 processes for the earlier WebView2 build).

## Features

**WizTree parity**
- Direct `$MFT` scanning (admin), with a parallel folder walk fallback for non-NTFS drives or non-admin runs
- Tree view: Allocated / Size / % of parent / Files / Folders / Modified, sortable, keyboard navigable, virtualized (any folder size)
- Treemap with per-extension colors, cushion shading, hover inspector, click-to-select, double-click to zoom
- File types panel (click to highlight in the treemap, double-click to list those files)
- Largest files / folders with filters: wildcards (`*.mp4`), substrings, path patterns (`\Downloads\*`), `ext:iso`, minimum size
- Allocated vs. logical size toggle (sparse/compressed aware)
- Scan a single folder (uses the MFT and re-roots when possible); CSV export **and** import in WizTree's format
- Delete to the Recycle Bin or permanently, Open, Show in Explorer, Copy path, via the context menu or `Del` / `Shift+Del`

**Beyond WizTree**
- Sunburst view
- Duplicate finder (same name + exact size), ranked by wasted space
- Smart Cleanup: detects temp folders, shader caches, browser caches, package-manager caches, crash dumps, Windows Update downloads and the Recycle Bin; empties them safely
- "Older than" filter (30 days → 3 years) for finding stale files
- Cancelable scans with a live progress overlay
- "Analyze with PrismDisk" in Explorer's right-click menu for folders and drives
- Restart-as-administrator button when running unelevated
- Refuses to delete critical system locations (Windows, Program Files, user profile roots, drive roots)

## Layout

```
PrismDisk/
├── Cargo.toml              # workspace (app + setup), shared release profile
├── app/                    # PrismDisk.exe (egui)
│   ├── src/mft.rs          # NTFS MFT reader (fixups, data runs, extension records)
│   ├── src/scan.rs         # scan control, parallel folder walk, WizTree CSV import/export
│   ├── src/tree.rs         # compact arena tree + queries (children, find, treemap, sunburst, dups, cleanup)
│   ├── src/backend.rs      # scan orchestration, delete (with system-folder protection), export
│   ├── src/app.rs          # app state, background jobs, actions
│   ├── src/ui.rs           # every screen, drawn to match the original design
│   ├── src/viz.rs          # treemap / sunburst GPU meshes + hit testing
│   ├── src/theme.rs        # colors, fonts (Segoe UI / Cascadia Mono), gradients, widgets
│   └── src/icons.rs        # SVG icon set
├── setup/                  # PrismDisk-Setup.exe (egui wizard, requires admin)
│   ├── src/main.rs         # the wizard UI (reuses app's theme + icons)
│   ├── src/install.rs      # install/uninstall: files, shortcuts, registry, silent mode
│   └── build.rs            # gzip-embeds target\release\PrismDisk.exe into the setup
├── installer/license.txt   # license shown by the wizard
└── build.ps1               # one-command release build → dist\PrismDisk-Setup-<ver>.exe
```

Scans run on background threads; the tree stays in a shared `RwLock`, and the UI asks it only for what is on screen (one folder's children, a treemap layout, top-N lists). Treemaps are a single cached vertex mesh, so even 250k rectangles cost one draw call.

## Building

Prerequisites: Rust (MSVC toolchain). Nothing else.

```powershell
.\build.ps1              # app + installer → dist\PrismDisk-Setup-1.0.0.exe
.\build.ps1 -SkipApp     # rebuild only the installer
cargo run -p prismdisk   # run the app (from an elevated terminal for MFT scans)
cargo run -p prismdisk -- "D:\Games"     # open straight into a scan
cargo test -p prismdisk  # unit tests;  add `--release -- --ignored --nocapture` for a real C: scan (elevated)
```

Settings (size mode, view, panel height) are stored in `%APPDATA%\PrismDisk\settings.ini`.

## Installer

`PrismDisk-Setup-<ver>.exe` is a single self-contained executable with a custom wizard: Welcome → License → Options (location with live free-space check, Start menu / desktop shortcuts, Explorer integration, always-run-as-admin) → animated install → Finish. It detects existing installs and updates in place. The installed `Uninstall PrismDisk.exe` (also reachable from *Settings → Apps*) uses the same design.

Unattended:

```powershell
PrismDisk-Setup-1.0.0.exe --silent [--dir=C:\Path] [--no-desktop] [--no-context-menu] [--no-admin]
"Uninstall PrismDisk.exe" --uninstall --silent [--keep-data]
```

## Releasing

1. Bump `version` in the root `Cargo.toml`.
2. Add a `## x.y.z (date)` section at the top of `CHANGELOG.md` and commit.
3. Run `.\release.ps1`. It builds the installer, tags `vx.y.z`, pushes, and publishes a GitHub release with that changelog section as the notes.

Installed copies of PrismDisk check the latest GitHub release on launch, download the new installer in the background and offer **Restart & update**. The update runs the installer silently with the options the user originally chose.

## Website

The download site lives in `docs/` and is plain static HTML. It is served by GitHub Pages at https://luaxii.github.io/PrismDisk/ and can also be deployed to Vercel as-is (`vercel.json` points Vercel at `docs/`, no build step). The download button, version and changelog are read live from GitHub releases, with `docs/CHANGELOG.md` as a fallback.

## License

MIT, see [installer/license.txt](installer/license.txt).
