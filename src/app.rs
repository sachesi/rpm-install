use adw::prelude::*;
use gtk::glib;
use tracing::{error, info, warn};

use crate::backend::{BackendOperation, install_local_file};
use crate::error::{AppError, AppResult};
use crate::installed_state::detect_installed;
use crate::rpm_info::{RpmInfo, canonicalize_and_validate, read_rpm_info};
use crate::state_logic::{ActionMode, action_for_relation, backend_operation_for_relation};
use crate::ui::Ui;

const APP_ID: &str = "com.example.RpmInstallerGui";

pub fn run() {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_activate(|app| {
        let ui = Ui::new(app);
        ui.status_label
            .set_label("Open an RPM file from Nautilus or pass one on the command line.");
        ui.install_button.set_sensitive(false);
        ui.window.present();
    });

    app.connect_open(|app, files, _hint| {
        if files.is_empty() {
            show_error_window(app, "No file provided.");
            return;
        }

        if files.len() > 1 {
            warn!("Multiple files passed, using first only");
        }

        let Some(path_input) = file_to_input(&files[0]) else {
            show_error_window(app, "Could not read file path from the desktop shell.");
            return;
        };

        if let Err(err) = build_main_window(app, &path_input, files.len() > 1) {
            error!("Could not build app window: {err}");
            show_error_window(app, &err.to_string());
        }
    });

    app.run();
}

fn file_to_input(file: &gio::File) -> Option<String> {
    if file.is_native() {
        return file.path().map(|p| p.display().to_string());
    }
    Some(file.uri().to_string())
}

fn show_error_window(app: &adw::Application, message: &str) {
    let ui = Ui::new(app);
    ui.install_button.set_sensitive(false);
    ui.show_error(message);
    ui.window.present();
}

fn build_main_window(
    app: &adw::Application,
    path_input: &str,
    warned_multi: bool,
) -> AppResult<()> {
    let path = canonicalize_and_validate(path_input)?;
    let info = read_rpm_info(&path)?;
    let installed = detect_installed(&info)?;

    let ui = Ui::new(app);
    ui.bind_package(&info, &installed);
    ui.window.present();

    if warned_multi {
        ui.toast("Multiple files were provided; showing the first one.");
    }

    let action_mode = action_for_relation(&installed.relation);
    let backend_operation = backend_operation_for_relation(&installed.relation);

    wire_install_action(&ui, info, action_mode, backend_operation);

    Ok(())
}

fn wire_install_action(
    ui: &Ui,
    info: RpmInfo,
    action_mode: ActionMode,
    backend_operation: BackendOperation,
) {
    let ui_cloned = ui.clone();

    ui.install_button.connect_clicked(move |_| {
        ui_cloned.hide_error();

        let (heading, action_label, body) = match action_mode {
            ActionMode::Install => (
                "Confirm install",
                "Install",
                "This action uses Fedora's dnf5daemon backend and may require administrator authentication.",
            ),
            ActionMode::Reinstall => (
                "Confirm reinstall",
                "Reinstall",
                "This exact build is already installed. The transaction will request a true dnf5daemon reinstall and may require administrator authentication.",
            ),
            ActionMode::Downgrade => (
                "Confirm downgrade",
                "Install",
                "A newer version is already installed. Continue to downgrade to this local RPM using dnf5daemon?",
            ),
        };

        let confirm = adw::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .build();

        confirm.add_response("cancel", "Cancel");
        confirm.add_response("ok", action_label);
        confirm.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
        confirm.set_default_response(Some("cancel"));
        confirm.set_close_response("cancel");

        let ui_for_response = ui_cloned.clone();
        let info_for_response = info.clone();
        confirm.choose(Some(&ui_cloned.window), gio::Cancellable::NONE, move |response| {
            if response != "ok" {
                ui_for_response.toast("Installation canceled.");
                return;
            }

            ui_for_response.set_busy(true);
            ui_for_response.status_label.set_label("Running installation via dnf5daemon…");

            let ui_for_async = ui_for_response.clone();
            let path = info_for_response.path.display().to_string();

            glib::MainContext::default().spawn_local(async move {
                let ui_for_progress = ui_for_async.clone();
                let result = install_local_file(&path, backend_operation, move |pct| {
                    ui_for_progress.set_progress(pct);
                })
                .await;

                match result {
                    Ok(done) => {
                        ui_for_async.set_busy(false);
                        let msg = match done {
                            BackendOperation::Reinstall => "Reinstalled successfully",
                            BackendOperation::Downgrade => "Installed (downgrade) successfully",
                            BackendOperation::Upgrade | BackendOperation::Install => "Installed successfully",
                        };
                        ui_for_async.status_label.set_label(msg);
                        ui_for_async.toast(msg);
                        info!("{msg}: {path}");

                        let win = ui_for_async.window.clone();
                        glib::timeout_add_seconds_local_once(2, move || {
                            win.close();
                        });
                    }
                    Err(AppError::TransactionCanceled | AppError::AuthCanceled) => {
                        ui_for_async.show_canceled("Installation canceled.");
                    }
                    Err(err) => {
                        ui_for_async.show_error(&humanize_error(&err));
                        error!("Install failed: {err}");
                    }
                }
            });
        });
    });
}

fn humanize_error(error: &AppError) -> String {
    match error {
        AppError::UnsupportedFileType => "Only local .rpm files are supported.".to_string(),
        AppError::DirectoryNotSupported => "Please choose an RPM file, not a directory.".to_string(),
        AppError::DaemonUnavailable(_) => {
            "dnf5daemon is unavailable. Ensure dnf5daemon-server is installed and running on Fedora.".to_string()
        }
        AppError::AuthCanceled => "Authentication was canceled or denied.".to_string(),
        AppError::TransactionCanceled => "Transaction canceled.".to_string(),
        AppError::UnsupportedOperation(details) => format!("Unsupported backend operation: {details}"),
        AppError::InstallFailure(details) => format!("Installation failed: {details}"),
        other => other.to_string(),
    }
}
