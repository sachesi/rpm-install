# RPM Installer GUI (GTK4 + libadwaita + dnf5daemon)

Fedora-specific desktop app in Rust for opening a local `.rpm`, showing clean package metadata, and running install/reinstall/upgrade/downgrade via `dnf5daemon` over D-Bus.

## Fedora scope

This project is intentionally Fedora-specific:

- Backend API target: `org.rpm.dnf.v0` (`dnf5daemon`) on system D-Bus.
- UI toolkit target: GNOME GTK4 + libadwaita.
- Installed-state logic: RPM EVR semantics and architecture awareness.

## Why dnf5daemon (and not PackageKit)

The app now uses Fedora-native `dnf5daemon` D-Bus APIs instead of PackageKit.

Benefits:

- Uses explicit dnf5 transaction operations (`install`, `reinstall`, `upgrade`, `downgrade`).
- Clearer error handling categories for daemon/auth/cancel/unsupported/failure.
- Session-based transaction flow (`open_session` → mark operation → `resolve` → `do_transaction` → `close_session`).

## Runtime requirements

- Fedora with `dnf5daemon` server installed.
- System D-Bus available.
- Polkit agent available for authentication prompts.
- GTK4/libadwaita runtime.

### Service behavior

`dnf5daemon` is treated as D-Bus activation-based. The app does **not** require or instruct `systemctl enable dnf5daemon-server.service`.

If daemon activation fails or the service is missing, the app reports it as a daemon-unavailable error.

## Local RPM operation behavior

For local RPM input, operation selection is driven by installed-state classification:

- `NotInstalled` → backend operation `install` (button: Install)
- `SameVersion` → backend operation `reinstall` (button: Reinstall)
- `Upgrade` → backend operation `upgrade` (button: Install)
- `Downgrade` → backend operation `downgrade` (button: Install + explicit confirmation)

No fake reinstall/downgrade labeling is used.

## UI design notes

The main window was redesigned to follow libadwaita patterns:

- `adw::Application` + `adw::ApplicationWindow`
- Integrated `adw::ToolbarView` + `adw::HeaderBar` (no detached/seam look)
- Compact `adw::Clamp` layout
- Structured groups (`adw::PreferencesGroup`, `adw::ActionRow`, `adw::ExpanderRow`)
- `gtk::Revealer` for transient progress and status
- `adw::ToastOverlay` for success/toast feedback
- `adw::StatusPage` for readable cancellation/error/success presentation

Theme handling uses `AdwStyleManager::set_color_scheme` and does not use `GtkSettings:gtk-application-prefer-dark-theme`.

## Build & run

```bash
cargo build
cargo run -- /path/to/package.rpm
```

URI input is also accepted:

```bash
cargo run -- file:///home/user/Downloads/example.rpm
```

## Test commands

```bash
cargo fmt
cargo check
cargo clippy --all-targets --all-features
cargo test
```
