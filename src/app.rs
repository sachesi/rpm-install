use std::path::PathBuf;

use adw::prelude::*;
use gio::prelude::*;
use gtk::glib;
use tracing::{error, info, warn};

use crate::error::{AppError, AppResult};
use crate::installed_state::detect_installed;
use crate::packagekit::{InstallMode, install_local_file};
use crate::rpm_info::{RpmInfo, canonicalize_and_validate, read_rpm_info};
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

    let mode = if installed.installed {
        InstallMode::Reinstall
    } else {
        InstallMode::Install
    };

    wire_install_action(&ui, info, mode);

    Ok(())
}

fn wire_install_action(ui: &Ui, info: RpmInfo, mode: InstallMode) {
    let ui_cloned = ui.clone();

    ui.install_button.connect_clicked(move |_| {
        ui_cloned.hide_error();

        let mode_label = match mode {
            InstallMode::Install => "install",
            InstallMode::Reinstall => "reinstall",
        };
        let confirm = adw::AlertDialog::builder()
            .heading(format!("Confirm {mode_label}"))
            .body("This action will use PackageKit and may require administrator authentication.")
            .build();

        confirm.add_response("cancel", "Cancel");
        confirm.add_response("ok", &mode_label.to_uppercase_first());
        confirm.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        confirm.set_default_response(Some("ok"));
        confirm.set_close_response("cancel");

        let ui_for_response = ui_cloned.clone();
        let info_for_response = info.clone();
        confirm.choose(
            Some(&ui_cloned.window),
            gio::Cancellable::NONE,
            move |response| {
                if response != "ok" {
                    ui_for_response.toast("Installation canceled.");
                    return;
                }

                ui_for_response.set_busy(true);
                ui_for_response
                    .status_label
                    .set_label("Running installation via PackageKit…");

                let ui_for_async = ui_for_response.clone();
                let path = info_for_response.path.display().to_string();
                glib::MainContext::default().spawn_local(async move {
                    let ui_for_progress = ui_for_async.clone();
                    let result = install_local_file(&path, mode, move |pct| {
                        ui_for_progress.set_progress(pct);
                    })
                    .await;

                    match result {
                        Ok(done) => {
                            ui_for_async.set_busy(false);
                            let msg = if done.reinstalled {
                                "Reinstalled successfully"
                            } else {
                                "Installed successfully"
                            };
                            ui_for_async.status_label.set_label(msg);
                            ui_for_async.toast(msg);
                            info!("{msg}: {}", path);

                            let win = ui_for_async.window.clone();
                            glib::timeout_add_seconds_local_once(2, move || {
                                win.close();
                            });
                        }
                        Err(AppError::InstallationCanceled) => {
                            ui_for_async.show_error("Installation canceled.");
                        }
                        Err(err) => {
                            ui_for_async.show_error(&humanize_error(&err));
                            error!("Install failed: {err}");
                        }
                    }
                });
            },
        );
    });

    if info.is_source_rpm() {
        ui.install_button.set_sensitive(false);
        ui.show_error("Source RPMs are not installable with this GUI.");
    }
}

fn humanize_error(error: &AppError) -> String {
    match error {
        AppError::UnsupportedFileType => "Only local .rpm files are supported.".to_string(),
        AppError::DirectoryNotSupported => {
            "Please choose an RPM file, not a directory.".to_string()
        }
        AppError::InstallationCanceled => "Installation canceled.".to_string(),
        AppError::PackageKit { details, .. } => format!("Installation failed: {details}"),
        other => other.to_string(),
    }
}

trait UppercaseFirst {
    fn to_uppercase_first(&self) -> String;
}

impl UppercaseFirst for str {
    fn to_uppercase_first(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
            None => String::new(),
        }
    }
}
