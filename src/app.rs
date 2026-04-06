use adw::prelude::*;
use gtk::glib;
use tracing::{error, info, warn};

use crate::backend::dnf5daemon::run_local_rpm_transaction;
use crate::backend::types::{BackendOperation, operation_for_relation};
use crate::error::{AppError, AppResult};
use crate::installed_state::detect_installed;
use crate::rpm_info::{RpmInfo, canonicalize_and_validate, read_rpm_info};
use crate::state_logic::InstallRelation;
use crate::ui::Ui;

const APP_ID: &str = "com.example.RpmInstallerGui";

pub fn run() {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
    });

    app.connect_activate(|app| {
        let ui = Ui::new(app);
        ui.action_button.set_sensitive(false);
        ui.show_status(
            "document-open-recent-symbolic",
            "Open a local RPM",
            "Open an .rpm file from Files or pass one on the command line.",
            None,
        );
        ui.window.present();
    });

    app.connect_open(|app, files, _hint| {
        if files.is_empty() {
            show_error_window(app, "No file was provided.");
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
    ui.action_button.set_sensitive(false);
    ui.show_status(
        "dialog-error-symbolic",
        "Cannot open RPM",
        message,
        Some("error"),
    );
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
    let operation = operation_for_relation(&installed.relation);

    let ui = Ui::new(app);
    ui.bind_package(&info, &installed, operation);
    ui.hide_status();
    ui.window.present();

    if warned_multi {
        ui.toast("Multiple files were provided; showing the first one.");
    }

    wire_install_action(&ui, info, operation, installed.relation.clone());

    Ok(())
}

fn wire_install_action(
    ui: &Ui,
    info: RpmInfo,
    operation: BackendOperation,
    relation: InstallRelation,
) {
    let ui_cloned = ui.clone();

    ui.action_button.connect_clicked(move |_| {
        ui_cloned.hide_status();

        let (heading, body, destructive) = match relation {
            InstallRelation::Downgrade => (
                "Confirm downgrade",
                "A newer version is already installed. Continue and downgrade to this local RPM?",
                true,
            ),
            InstallRelation::SameVersion => (
                "Confirm reinstall",
                "This exact version is already installed. Continue with a reinstall?",
                false,
            ),
            InstallRelation::Upgrade => (
                "Confirm install",
                "An older version is installed. Continue to upgrade using this local RPM?",
                false,
            ),
            InstallRelation::NotInstalled => {
                ("Confirm install", "Install this local RPM package?", false)
            }
        };

        let confirm = adw::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .build();
        confirm.add_response("cancel", "Cancel");
        confirm.add_response(
            "ok",
            ui_cloned
                .action_button
                .label()
                .as_deref()
                .unwrap_or("Install"),
        );
        if destructive {
            confirm.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
        } else {
            confirm.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        }
        confirm.set_default_response(Some("cancel"));
        confirm.set_close_response("cancel");

        let ui_for_response = ui_cloned.clone();
        let info_for_response = info.clone();
        confirm.choose(
            Some(&ui_cloned.window),
            gio::Cancellable::NONE,
            move |response| {
                if response != "ok" {
                    ui_for_response.show_status(
                        "process-stop-symbolic",
                        "Canceled",
                        "Transaction was canceled before it started.",
                        None,
                    );
                    return;
                }

                ui_for_response.set_running(true);

                let ui_for_async = ui_for_response.clone();
                let path = info_for_response.path.display().to_string();
                glib::MainContext::default().spawn_local(async move {
                    let ui_for_progress = ui_for_async.clone();
                    let result = run_local_rpm_transaction(&path, operation, move |progress| {
                        ui_for_progress.set_progress(progress);
                    })
                    .await;

                    ui_for_async.set_running(false);

                    match result {
                        Ok(done) => {
                            let msg = format!("{} successfully", done.verb_past());
                            ui_for_async.show_status(
                                "emblem-ok-symbolic",
                                "Transaction complete",
                                &msg,
                                Some("success"),
                            );
                            ui_for_async.toast(&msg);
                            info!("{}: {}", msg, path);

                            let win = ui_for_async.window.clone();
                            glib::timeout_add_seconds_local_once(2, move || {
                                win.close();
                            });
                        }
                        Err(err) => {
                            let human = humanize_error(&err, operation);
                            let icon = if matches!(
                                err,
                                AppError::TransactionCanceled | AppError::AuthCanceled
                            ) {
                                "process-stop-symbolic"
                            } else {
                                "dialog-error-symbolic"
                            };
                            let css = if matches!(
                                err,
                                AppError::TransactionCanceled | AppError::AuthCanceled
                            ) {
                                None
                            } else {
                                Some("error")
                            };
                            ui_for_async.show_status(
                                icon,
                                "Transaction did not complete",
                                &human,
                                css,
                            );
                            error!("Install flow failed: {err}");
                        }
                    }
                });
            },
        );
    });
}

fn humanize_error(error: &AppError, operation: BackendOperation) -> String {
    match error {
        AppError::UnsupportedFileType => "Only local .rpm files are supported.".to_string(),
        AppError::DirectoryNotSupported => {
            "Please choose an RPM file, not a directory.".to_string()
        }
        AppError::DaemonUnavailable(_) => "dnf5daemon is not available on system D-Bus. Install the dnf5daemon packages and try again. The service is D-Bus activated and does not need manual systemctl enable.".to_string(),
        AppError::AuthDenied => "Authentication was denied; no changes were made.".to_string(),
        AppError::AuthCanceled => "Authentication was canceled.".to_string(),
        AppError::TransactionCanceled => "The transaction was canceled.".to_string(),
        AppError::UnsupportedOperation(_) => format!(
            "This system's dnf5daemon runtime does not support {:?} for the selected local RPM.",
            operation
        ),
        AppError::InvalidLocalRpm(details) => format!("The selected RPM is unreadable or invalid: {details}"),
        AppError::OperationFailed { details, .. } => details.clone(),
        other => other.to_string(),
    }
}
