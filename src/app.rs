use adw::prelude::*;
use gtk::glib;
use tracing::{error, info, warn};

use crate::backend::{preview_local_rpm_transaction, run_local_rpm_transaction};
use crate::backend::types::{BackendOperation, TransactionPreview, operation_for_relation};
use crate::error::{AppError, AppResult};
use crate::installed_state::detect_installed;
use crate::rpm_info::{RpmInfo, canonicalize_and_validate, read_rpm_info};
use crate::state_logic::{ActionMode, InstallRelation, action_for_relation};
use crate::ui::Ui;

const APP_ID: &str = "com.github.sachesi.rpminstall";

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
    let action_mode = action_for_relation(&installed.relation);
    let package_name = info.name.clone();

    let ui = Ui::new(app);
    ui.bind_package(&info, &installed, action_mode);
    ui.hide_status();
    ui.window.present();

    if warned_multi {
        ui.show_status(
            "dialog-information-symbolic",
            "Multiple files detected",
            "Showing the first selected file.",
            None,
        );
    }

    wire_install_action(
        &ui,
        info,
        operation,
        action_mode,
        installed.relation.clone(),
    );
    if !matches!(installed.relation, InstallRelation::NotInstalled) {
        wire_uninstall_action(&ui, &installed.relation, package_name);
    }

    Ok(())
}

fn wire_install_action(
    ui: &Ui,
    info: RpmInfo,
    operation: BackendOperation,
    action_mode: ActionMode,
    relation: InstallRelation,
) {
    let ui_cloned = ui.clone();

    ui.action_button.connect_clicked(move |_| {
        let ui_for_preview = ui_cloned.clone();
        let info_for_preview = info.clone();
        let relation_for_preview = relation.clone();

        glib::MainContext::default().spawn_local(async move {
            ui_for_preview.hide_status();
            ui_for_preview.set_running(true);

            let path = info_for_preview.path.display().to_string();
            let preview_result = preview_local_rpm_transaction(&path, operation).await;

            ui_for_preview.set_running(false);

            match preview_result {
                Ok(preview) => present_install_confirmation(
                    ui_for_preview.clone(),
                    info_for_preview.clone(),
                    operation,
                    action_mode,
                    relation_for_preview.clone(),
                    preview,
                ),
                Err(err) => {
                    let human = humanize_error(&err, operation);
                    ui_for_preview.show_status(
                        "dialog-error-symbolic",
                        "Could not review dependencies",
                        &human,
                        Some("error"),
                    );
                    error!("Dependency preview failed: {err}");
                }
            }
        });
    });
}

fn present_install_confirmation(
    ui: Ui,
    info: RpmInfo,
    operation: BackendOperation,
    action_mode: ActionMode,
    relation: InstallRelation,
    preview: TransactionPreview,
) {
    let confirm_copy = relation.confirmation_copy();
    let confirm = adw::AlertDialog::builder()
        .heading(confirm_copy.heading)
        .body(build_confirmation_body(confirm_copy.body, &preview))
        .build();
    let confirm_action_label = match action_mode {
        ActionMode::Install | ActionMode::Downgrade => "Install",
        ActionMode::Reinstall => "Reinstall",
        ActionMode::Upgrade => "Upgrade",
    };
    confirm.add_response("cancel", "Cancel");
    confirm.add_response("ok", confirm_action_label);
    if confirm_copy.destructive {
        confirm.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
    } else {
        confirm.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    }
    confirm.set_default_response(Some("cancel"));
    confirm.set_close_response("cancel");

    let ui_for_response = ui.clone();
    confirm.choose(Some(&ui.window), gio::Cancellable::NONE, move |response| {
        if response != "ok" {
            ui_for_response.show_status(
                "process-stop-symbolic",
                "Canceled",
                "Transaction was canceled before it started.",
                None,
            );
            return;
        }

        let path = info.path.display().to_string();
        run_confirmed_transaction(
            ui_for_response.clone(),
            path,
            operation,
            "Install flow failed",
        );
    });
}

fn wire_uninstall_action(ui: &Ui, relation: &InstallRelation, package_name: String) {
    let relation = relation.clone();
    let ui_cloned = ui.clone();
    ui.uninstall_button.connect_clicked(move |_| {
        ui_cloned.hide_status();

        let body = match &relation {
            InstallRelation::SameVersion => {
                "The currently installed package version will be removed."
            }
            InstallRelation::Upgrade | InstallRelation::Downgrade => {
                "The currently installed package will be removed from the system."
            }
            InstallRelation::NotInstalled => return,
        };

        let confirm = adw::AlertDialog::builder()
            .heading("Confirm uninstall")
            .body(body)
            .build();
        confirm.add_response("cancel", "Cancel");
        confirm.add_response("ok", "Uninstall");
        confirm.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
        confirm.set_default_response(Some("cancel"));
        confirm.set_close_response("cancel");

        let ui_for_response = ui_cloned.clone();
        let pkg_for_response = package_name.clone();
        confirm.choose(
            Some(&ui_cloned.window),
            gio::Cancellable::NONE,
            move |response| {
                if response != "ok" {
                    ui_for_response.show_status(
                        "process-stop-symbolic",
                        "Canceled",
                        "Uninstall was canceled before it started.",
                        None,
                    );
                    return;
                }

                run_confirmed_transaction(
                    ui_for_response.clone(),
                    pkg_for_response.clone(),
                    BackendOperation::Remove,
                    "Uninstall flow failed",
                );
            },
        );
    });
}

fn run_confirmed_transaction(
    ui: Ui,
    spec: String,
    operation: BackendOperation,
    log_prefix: &'static str,
) {
    ui.set_running(true);

    glib::MainContext::default().spawn_local(async move {
        let ui_for_progress = ui.clone();
        let result = run_local_rpm_transaction(&spec, operation, move |progress| {
            ui_for_progress.set_progress(progress);
        })
        .await;

        ui.set_running(false);

        match result {
            Ok(done) => {
                let msg = format!("{} successfully", done.verb_past());
                ui.show_toast(&msg);
                info!("{}: {}", msg, spec);

                let win = ui.window.clone();
                glib::timeout_add_seconds_local_once(2, move || {
                    win.close();
                });
            }
            Err(err) => {
                let human = humanize_error(&err, operation);
                let icon = if matches!(err, AppError::TransactionCanceled | AppError::AuthCanceled)
                {
                    "process-stop-symbolic"
                } else {
                    "dialog-error-symbolic"
                };
                let css = if matches!(err, AppError::TransactionCanceled | AppError::AuthCanceled) {
                    None
                } else {
                    Some("error")
                };
                ui.show_status(icon, "Transaction did not complete", &human, css);
                error!("{log_prefix}: {err}");
            }
        }
    });
}

fn humanize_error(error: &AppError, operation: BackendOperation) -> String {
    const MAX_ERROR_CHARS: usize = 280;

    match error {
        AppError::NonLocalPath(_) => "Only local files are supported. Please select a local .rpm file.".to_string(),
        AppError::UnsupportedFileType => "Only local .rpm files are supported.".to_string(),
        AppError::DirectoryNotSupported => {
            "Please choose an RPM file, not a directory.".to_string()
        }
        AppError::SourceRpmNotInstallable => {
            "Source RPM files (.src.rpm/.nosrc.rpm) cannot be installed with this GUI.".to_string()
        }
        AppError::DaemonUnavailable(_) => "dnf5daemon is not available on system D-Bus. Install the dnf5daemon packages and try again. The service is D-Bus activated and does not need manual systemctl enable.".to_string(),
        AppError::AuthDenied => "Authentication was denied; no changes were made.".to_string(),
        AppError::AuthCanceled => "Authentication was canceled.".to_string(),
        AppError::TransactionCanceled => "The transaction was canceled.".to_string(),
        AppError::UnsupportedOperation(_) => format!(
            "This system's dnf5daemon runtime does not support {:?} for the selected local RPM.",
            operation
        ),
        AppError::NonRegularFile => "Please choose a regular .rpm file.".to_string(),
        AppError::InvalidLocalRpm(details) => format!(
            "The selected RPM is unreadable or invalid: {}",
            clamp_message(details, MAX_ERROR_CHARS)
        ),
        AppError::OperationFailed { details, .. } => clamp_message(details, MAX_ERROR_CHARS),
        AppError::Other(_) => "Unexpected internal failure. Please retry and check logs for details.".to_string(),
    }
}

fn clamp_message(message: &str, max_chars: usize) -> String {
    let mut trimmed = message.trim().chars().take(max_chars).collect::<String>();
    if message.chars().count() > max_chars {
        trimmed.push('…');
    }
    trimmed
}

fn build_confirmation_body(base: &str, preview: &TransactionPreview) -> String {
    if preview.additional_package_changes.is_empty() {
        return format!("{base}\n\nNo additional packages are required for this transaction.");
    }

    const MAX_ITEMS: usize = 8;

    let shown = preview
        .additional_package_changes
        .iter()
        .take(MAX_ITEMS)
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let remaining = preview
        .additional_package_changes
        .len()
        .saturating_sub(MAX_ITEMS);
    let remainder = if remaining == 0 {
        String::new()
    } else {
        format!("\n...and {remaining} more.")
    };

    format!("{base}\n\nAdditional packages required for this transaction:\n{shown}{remainder}")
}

#[cfg(test)]
mod tests {
    use super::{TransactionPreview, build_confirmation_body};

    #[test]
    fn confirmation_body_mentions_when_no_extra_packages_are_needed() {
        let body = build_confirmation_body(
            "Install this local RPM package?",
            &TransactionPreview::default(),
        );

        assert!(body.contains("No additional packages are required"));
    }

    #[test]
    fn confirmation_body_limits_long_dependency_lists() {
        let preview = TransactionPreview {
            additional_package_changes: (1..=10).map(|idx| format!("Install dep{idx}")).collect(),
        };

        let body = build_confirmation_body("Install this local RPM package?", &preview);

        assert!(body.contains("Additional packages required for this transaction"));
        assert!(body.contains("- Install dep1"));
        assert!(body.contains("- Install dep8"));
        assert!(body.contains("...and 2 more."));
        assert!(!body.contains("- Install dep9"));
    }
}
