use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::pango;
use gtk::{Align, Orientation};

use crate::installed_state::InstalledState;
use crate::rpm_info::{RpmInfo, format_size};
use crate::state_logic::InstallRelation;

#[derive(Clone)]
pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub title_label: gtk::Label,
    pub subtitle_label: gtk::Label,
    pub status_label: gtk::Label,
    pub installed_context_label: gtk::Label,
    pub install_button: gtk::Button,
    pub spinner: gtk::Spinner,
    pub progress: gtk::ProgressBar,
    pub error_revealer: gtk::Revealer,
    pub error_label: gtk::Label,
    pub toast_overlay: adw::ToastOverlay,
    pub detail_rows: Rc<RefCell<Vec<adw::ActionRow>>>,
}

impl Ui {
    pub fn new(app: &adw::Application) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("RPM Installer")
            .default_width(620)
            .default_height(560)
            .resizable(false)
            .build();
        window.set_size_request(620, 560);

        let header = adw::HeaderBar::builder()
            .title_widget(&gtk::Label::builder().label("Local RPM Installer").build())
            .build();

        let title_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .css_classes(["title-2"])
            .build();

        let subtitle_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();

        let status_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .build();

        let installed_context_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .css_classes(["caption", "dim-label"])
            .build();

        let spinner = gtk::Spinner::builder()
            .halign(Align::Start)
            .spinning(false)
            .build();
        let progress = gtk::ProgressBar::builder()
            .hexpand(true)
            .show_text(true)
            .build();
        progress.set_fraction(0.0);
        progress.set_text(Some("Idle"));

        let install_button = gtk::Button::builder()
            .label("Install")
            .css_classes(["suggested-action"])
            .halign(Align::End)
            .build();

        let error_label = gtk::Label::builder().wrap(true).xalign(0.0).build();
        let error_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(["error"])
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        error_box.append(&gtk::Image::from_icon_name("dialog-error-symbolic"));
        error_box.append(&error_label);

        let error_revealer = gtk::Revealer::builder().reveal_child(false).build();
        error_revealer.set_child(Some(&error_box));

        let details_group = adw::PreferencesGroup::builder()
            .title("Package details")
            .build();

        let detail_rows = Rc::new(RefCell::new(Vec::<adw::ActionRow>::new()));
        let add_row = |group: &adw::PreferencesGroup,
                       rows: &Rc<RefCell<Vec<adw::ActionRow>>>,
                       title: &str,
                       value: String| {
            let subtitle = gtk::Label::builder()
                .label(value)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .xalign(0.0)
                .selectable(true)
                .build();
            subtitle.set_max_width_chars(60);
            subtitle.set_ellipsize(pango::EllipsizeMode::End);

            let row = adw::ActionRow::builder().title(title).build();
            row.add_suffix(&subtitle);
            row.set_activatable(false);
            rows.borrow_mut().push(row.clone());
            group.add(&row);
        };

        add_row(&details_group, &detail_rows, "Name", String::new());
        add_row(&details_group, &detail_rows, "Version", String::new());
        add_row(&details_group, &detail_rows, "Architecture", String::new());
        add_row(&details_group, &detail_rows, "Summary", String::new());
        add_row(&details_group, &detail_rows, "Description", String::new());
        add_row(&details_group, &detail_rows, "License", String::new());
        add_row(&details_group, &detail_rows, "Vendor", String::new());
        add_row(&details_group, &detail_rows, "Packager", String::new());
        add_row(&details_group, &detail_rows, "Homepage", String::new());
        add_row(
            &details_group,
            &detail_rows,
            "Installed size",
            String::new(),
        );
        add_row(&details_group, &detail_rows, "Package size", String::new());
        add_row(&details_group, &detail_rows, "Source RPM", String::new());
        add_row(&details_group, &detail_rows, "Path", String::new());
        add_row(&details_group, &detail_rows, "Signature", String::new());

        let expander = adw::ExpanderRow::builder()
            .title("Metadata")
            .subtitle("Expand for full package metadata")
            .build();
        expander.add_row(&details_group);
        let expander_group = adw::PreferencesGroup::new();
        expander_group.add(&expander);

        let actions_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .halign(Align::Fill)
            .build();
        actions_box.append(&spinner);
        actions_box.append(&progress);
        actions_box.append(&install_button);

        let content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        content.append(&title_label);
        content.append(&subtitle_label);
        content.append(&status_label);
        content.append(&installed_context_label);
        content.append(&error_revealer);
        content.append(&expander_group);
        content.append(&actions_box);

        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .min_content_height(420)
            .child(&content)
            .build();

        let main_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .build();
        main_box.append(&header);
        main_box.append(&scroll);

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&main_box));
        window.set_content(Some(&toast_overlay));

        Self {
            window,
            title_label,
            subtitle_label,
            status_label,
            installed_context_label,
            install_button,
            spinner,
            progress,
            error_revealer,
            error_label,
            toast_overlay,
            detail_rows,
        }
    }

    pub fn bind_package(&self, info: &RpmInfo, installed: &InstalledState) {
        self.title_label
            .set_label(&format!("{} {}", info.name, info.version));
        self.subtitle_label
            .set_label(&format!("{} • {}", info.path.display(), info.arch));

        match installed.relation {
            InstallRelation::NotInstalled => {
                self.install_button.set_label("Install");
                self.status_label
                    .set_label("This package is not currently installed.");
            }
            InstallRelation::SameVersion => {
                self.install_button.set_label("Reinstall");
                self.status_label
                    .set_label("This exact package build is already installed.");
            }
            InstallRelation::Upgrade => {
                self.install_button.set_label("Install");
                self.status_label
                    .set_label("An older installed version was detected; this will update it.");
            }
            InstallRelation::Downgrade => {
                self.install_button.set_label("Install");
                self.status_label
                    .set_label("A newer installed version exists; this install may downgrade it.");
            }
        }

        self.installed_context_label.set_label(
            &installed
                .installed_evr_arch
                .as_ref()
                .map(|v| format!("Installed: {v}"))
                .unwrap_or_else(|| "Installed: not present".to_string()),
        );

        let rows = self.detail_rows.borrow();
        let values = vec![
            info.name.clone(),
            format!(
                "{}{}-{}",
                info.epoch.map(|e| format!("{e}:")).unwrap_or_default(),
                info.version,
                info.release
            ),
            info.arch.clone(),
            info.summary.clone().unwrap_or_else(|| "—".to_string()),
            info.description.clone().unwrap_or_else(|| "—".to_string()),
            info.license.clone().unwrap_or_else(|| "—".to_string()),
            info.vendor.clone().unwrap_or_else(|| "—".to_string()),
            info.packager.clone().unwrap_or_else(|| "—".to_string()),
            info.url.clone().unwrap_or_else(|| "—".to_string()),
            format_size(info.installed_size),
            format_size(info.package_size),
            info.source_rpm.clone().unwrap_or_else(|| "—".to_string()),
            info.path.display().to_string(),
            info.signature_status
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
        ];

        for (idx, row) in rows.iter().enumerate() {
            if let Some(suffix) = row.last_child().and_downcast::<gtk::Label>() {
                suffix.set_label(values.get(idx).map(String::as_str).unwrap_or("—"));
            }
        }
    }

    pub fn set_busy(&self, busy: bool) {
        self.install_button.set_sensitive(!busy);
        self.spinner.set_spinning(busy);
        if busy {
            self.progress.set_text(Some("Working…"));
            self.progress.pulse();
        }
    }

    pub fn set_progress(&self, pct: u32) {
        let fraction = (pct.min(100) as f64) / 100.0;
        self.progress.set_fraction(fraction);
        self.progress.set_text(Some(&format!("{pct}%")));
    }

    pub fn show_canceled(&self, message: &str) {
        self.hide_error();
        self.status_label.set_label(message);
        self.set_busy(false);
        self.toast(message);
    }

    pub fn show_error(&self, message: &str) {
        self.error_label.set_label(message);
        self.error_revealer.set_reveal_child(true);
        self.status_label.set_label("Installation failed.");
        self.set_busy(false);
    }

    pub fn hide_error(&self) {
        self.error_revealer.set_reveal_child(false);
    }

    pub fn toast(&self, message: &str) {
        self.toast_overlay.add_toast(adw::Toast::new(message));
    }
}
