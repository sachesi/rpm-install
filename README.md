# RPM Installer GUI (GTK4 + libadwaita + PackageKit)

A Linux-first desktop app in Rust for opening local `.rpm` files (including Nautilus `%f` handlers), showing metadata, and installing/reinstalling through PackageKit.

## Architecture summary

- **Frontend:** `adw::Application` with GTK4/libadwaita widgets and a fixed-size non-resizable `adw::ApplicationWindow`.
- **Open flow:** `gio::ApplicationFlags::HANDLES_OPEN` handles desktop-open and CLI file arguments.
- **RPM metadata:** parsed directly from the local RPM file with the `rpm` crate for immediate display.
- **Installed-state detection:** queries installed package EVR+arch and compares with local EVR using RPM version comparison semantics.
- **Install backend:** PackageKit D-Bus APIs (`CreateTransaction` + `InstallFiles`) via `packagekit-zbus`.
- **Reinstall behavior:**
  - Primary path: `InstallFiles` with PackageKit transaction flags `ALLOW_REINSTALL | JUST_REINSTALL`.
  - Native fallback path (when reinstall flags are not supported by the backend): resolve installed package id via PackageKit, remove through PackageKit `RemovePackages`, then install local file via PackageKit `InstallFiles`.

## Crates selected (latest compatible at implementation time)

- `gtk4 = 0.11.2`
- `libadwaita = 0.9.1` (renamed as `adw`)
- `gio = 0.22.4`
- `glib = 0.22.4`
- `zbus = 5.14.0`
- `packagekit-zbus = 0.2.0`
- `rpm = 0.19.0`
- `anyhow = 1.0.102`
- `thiserror = 2.0.18`
- `tracing = 0.1.44`
- `tracing-subscriber = 0.3.23`

## Fedora/GNOME compatibility assumptions

- Fedora-like system with:
  - `packagekitd` running on system D-Bus
  - PolicyKit agent available for admin authentication prompt
  - GNOME/GTK4/libadwaita runtime and dev packages installed
- Intended for local files only (`/path/*.rpm` and `file://` URIs that resolve to local/native files).

## Installed-state categories

The app distinguishes these states using EVR + arch comparison:

- **Not installed** → primary action: `Install`
- **Installed same build** → primary action: `Reinstall`
- **Installed older version** → primary action: `Install` (update behavior)
- **Installed newer version** → primary action: `Install` with explicit downgrade warning

A compact status line near the action area shows the detected installed build, e.g. `Installed: 1:1.2.3-1.fc44.x86_64`.

## Project layout

- `src/main.rs` – bootstrap + tracing
- `src/app.rs` – application wiring, file-open handling, install workflow
- `src/ui/mod.rs` – libadwaita UI composition/state helpers
- `src/rpm_info.rs` – path validation and RPM metadata extraction
- `src/packagekit.rs` – PackageKit D-Bus transaction logic
- `src/installed_state.rs` – installed-state detection logic
- `src/error.rs` – typed error model
- `packaging/com.example.RpmInstallerGui.desktop` – desktop file
- `packaging/com.example.RpmInstallerGui.metainfo.xml` – AppStream metadata

## Build & run

```bash
cargo build
cargo run -- /path/to/package.rpm
```

Example with URI input:

```bash
cargo run -- file:///home/user/Downloads/example.rpm
```

## Desktop integration (local install)

Install desktop integration files for your user:

```bash
install -Dm644 packaging/com.example.RpmInstallerGui.desktop ~/.local/share/applications/com.example.RpmInstallerGui.desktop
install -Dm644 packaging/com.example.RpmInstallerGui.metainfo.xml ~/.local/share/metainfo/com.example.RpmInstallerGui.metainfo.xml
update-desktop-database ~/.local/share/applications
```

Set default RPM opener if needed:

```bash
xdg-mime default com.example.RpmInstallerGui.desktop application/x-rpm
```

## Behavior highlights

- Accepts files from Nautilus (`%f`), CLI path, or `file://` URIs.
- Validates and canonicalizes local path.
- Rejects non-RPM files and directories with user-facing errors.
- Displays package metadata including summary/description/license/vendor/URL/sizes/path/signature hints.
- Uses **Install** vs **Reinstall** CTA based on installed-state check.
- Shows installed version context near actions.
- Shows progress and busy state during PackageKit transaction.
- On success, shows Installed/Reinstalled and auto-closes after ~2 seconds.
- On failure/cancel, keeps window open with a human-friendly error.

## Notes for Fedora RPM packaging later

- Install binary to `%{_bindir}/rpm-installer-gui`.
- Install desktop file to `%{_datadir}/applications/com.example.RpmInstallerGui.desktop`.
- Install metainfo file to `%{_datadir}/metainfo/com.example.RpmInstallerGui.metainfo.xml`.
- Add runtime deps for GTK4/libadwaita and PackageKit.
- Add MIME registration scriptlets as needed (`update-desktop-database`, `update-mime-database`).
- Consider providing app icon at standard hicolor paths under `%{_datadir}/icons/hicolor/...`.
