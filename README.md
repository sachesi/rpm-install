# RPM Installer GUI (GTK4 + libadwaita + dnf5daemon)

A Fedora-focused desktop app in Rust for opening local `.rpm` files, previewing metadata, and performing native installs via **dnf5daemon** over D-Bus.

## Why dnf5daemon (and not PackageKit)

Fedora is converging on the DNF5 stack. This app uses `dnf5daemon` directly so local-RPM transactions map to native DNF operations and policy handling:

- local install (`rpm.Rpm.install`)
- local reinstall (`rpm.Rpm.reinstall`)
- local upgrade (`rpm.Rpm.upgrade`)
- local downgrade (`rpm.Rpm.downgrade`)
- resolve + execute through `Goal.resolve` and `Goal.do_transaction`

This avoids PackageKit compatibility gaps and keeps behavior aligned with Fedora's current package-management backend.

## Fedora-only assumptions

This project is intentionally Fedora-specific:

- Requires system D-Bus service `org.rpm.dnf.v0` (`dnf5daemon-server`)
- Requires Polkit integration for privileged transactions
- Uses `rpm` CLI for installed-state detection and RPM EVR comparison semantics

If `dnf5daemon` is unavailable, the app reports a precise daemon-unavailable error and does not fall back to PackageKit.

## Architecture summary

- **Frontend:** GTK4 + libadwaita, fixed-size non-resizable `adw::ApplicationWindow`
- **Open flow:** `gio::ApplicationFlags::HANDLES_OPEN` (desktop open / CLI path)
- **RPM metadata:** parsed from the local file with the `rpm` crate
- **Installed-state detection:** local vs installed EVR/arch classified via `state_logic`
- **Action mapping:**
  - `NotInstalled` / `Upgrade` → **Install** button
  - `SameVersion` → **Reinstall** button
  - `Downgrade` → **Install** button + explicit downgrade warning
- **Backend mapping:**
  - `NotInstalled` → dnf5 `install`
  - `SameVersion` → dnf5 `reinstall`
  - `Upgrade` → dnf5 `upgrade`
  - `Downgrade` → dnf5 `downgrade`
- **Progress:** transaction progress is streamed from dnf5daemon signals and fed to the UI progress bar
- **Results:** success toast + auto-close, explicit cancellation/auth-cancel handling, precise backend errors

## Runtime dependencies

At runtime on Fedora you need:

- `dnf5daemon-server` (service exposing `org.rpm.dnf.v0`)
- A running D-Bus system bus
- A Polkit authentication agent
- GTK4 + libadwaita runtime libraries

## Crates used

- `gtk4 = 0.11.2`
- `libadwaita = 0.9.1` (as `adw`)
- `gio = 0.22.4`
- `glib = 0.22.4`
- `zbus = 5.14.0`
- `rpm = 0.19.0`
- `futures-util = 0.3.31`
- `anyhow = 1.0.102`
- `thiserror = 2.0.18`
- `tracing = 0.1.44`
- `tracing-subscriber = 0.3.23`

## Build & run

```bash
cargo build
cargo run -- /path/to/package.rpm
```

URI input is also supported:

```bash
cargo run -- file:///home/user/Downloads/example.rpm
```

## Project layout

- `src/main.rs` – bootstrap + tracing
- `src/app.rs` – application wiring and UX flow
- `src/backend/mod.rs` – backend module exports
- `src/backend/types.rs` – backend operation enum
- `src/backend/dnf5daemon.rs` – dnf5daemon D-Bus backend
- `src/state_logic.rs` – installed-state classification + action/backend mapping (+ tests)
- `src/installed_state.rs` – rpm query adapter
- `src/rpm_info.rs` – path validation + RPM metadata extraction
- `src/ui/mod.rs` – libadwaita UI
- `src/error.rs` – typed app errors

## Real Fedora validation checklist

Validate these scenarios on a Fedora host with `dnf5daemon-server` active:

1. Local RPM for package not installed
2. Local RPM matching exact installed EVR+arch (reinstall)
3. Local RPM newer than installed (upgrade)
4. Local RPM older than installed (downgrade)
5. Authentication canceled in Polkit prompt
6. Daemon unavailable (service stopped)

The app logs selected backend path (`install` / `upgrade` / `reinstall` / `downgrade`) via tracing to simplify runtime validation.
