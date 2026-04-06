use std::collections::HashMap;

use adw::prelude::*;
use gtk::pango;
use gtk::{Align, Orientation};

use crate::backend::types::BackendOperation;
use crate::installed_state::InstalledState;
use crate::rpm_info::{RpmInfo, format_size};
use crate::state_logic::{ActionMode, InstallRelation};

#[derive(Clone)]
pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub package_name_label: gtk::Label,
    pub version_label: gtk::Label,
    pub arch_label: gtk::Label,
    pub path_label: gtk::Label,
    pub state_row: adw::ActionRow,
    pub context_label: gtk::Label,
    pub status_revealer: gtk::Revealer,
    pub status_page: adw::StatusPage,
    pub progress_revealer: gtk::Revealer,
    pub spinner: gtk::Spinner,
    pub progress: gtk::ProgressBar,
    pub action_button: gtk::Button,
    pub toast_overlay: adw::ToastOverlay,
    detail_rows: Rc<RefCell<Vec<adw::ActionRow>>>,
}

impl Ui {
    pub fn new(app: &adw::Application) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("RPM Installer")
            .default_width(560)
            .default_height(600)
            .resizable(false)
            .build();
        window.set_size_request(560, 600);

        let header = adw::HeaderBar::builder()
            .show_end_title_buttons(true)
            .build();
        let header_title = adw::WindowTitle::builder()
            .title("Local RPM Installer")
            .subtitle("Fedora")
            .build();
        header.set_title_widget(Some(&header_title));

        let package_name_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .css_classes(["title-1"])
            .build();

        let version_label = secondary_label();
        let arch_label = secondary_label();
        let path_label = secondary_label();
        path_label.set_ellipsize(pango::EllipsizeMode::Middle);

        let identity_group = adw::PreferencesGroup::builder().title("Package").build();
        let identity_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        identity_box.append(&package_name_label);
        identity_box.append(&version_label);
        identity_box.append(&arch_label);
        identity_box.append(&path_label);
        identity_group.set_header_suffix(Some(&identity_box));

        let state_row = adw::ActionRow::builder().title("Installed state").build();
        let context_label = secondary_label();
        context_label.set_wrap(true);
        context_label.set_xalign(1.0);
        state_row.add_suffix(&context_label);
        state_row.set_activatable(false);

        let status_group = adw::PreferencesGroup::builder().title("Action").build();
        status_group.add(&state_row);

        let details_group = adw::PreferencesGroup::builder().title("Details").build();
        let expander = adw::ExpanderRow::builder()
            .title("Advanced metadata")
            .subtitle("Expand for package metadata")
            .build();
        content.append(&hero_block);
        content.append(&state_group);
        content.append(&details_group);

        let detail_rows = Rc::new(RefCell::new(Vec::<adw::ActionRow>::new()));
        for title in [
            "Summary",
            "Description",
            "License",
            "Vendor",
            "Packager",
            "Homepage",
            "Installed size",
            "Package size",
            "Source RPM",
            "Signature",
        ] {
            let row = adw::ActionRow::builder().title(title).build();
            let label = gtk::Label::builder()
                .xalign(1.0)
                .wrap(true)
                .max_width_chars(48)
                .ellipsize(pango::EllipsizeMode::End)
                .selectable(true)
                .build();
            row.add_suffix(&label);
            row.set_activatable(false);
            expander.add_row(&row);
            detail_rows.borrow_mut().push(row);
        }
        details_group.add(&expander);

        let status_page = adw::StatusPage::builder()
            .hexpand(true)
            .vexpand(false)
            .build();
        status_page.set_visible(false);
        let status_revealer = gtk::Revealer::builder().reveal_child(false).build();
        status_revealer.set_child(Some(&status_page));

        let spinner = gtk::Spinner::builder().spinning(false).build();
        let progress = gtk::ProgressBar::builder().hexpand(true).build();
        progress.set_show_text(true);

        let progress_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .build();
        progress_box.append(&spinner);
        progress_box.append(&progress);

        let progress_revealer = gtk::Revealer::builder().reveal_child(false).build();
        progress_revealer.set_child(Some(&progress_box));

        let action_button = gtk::Button::builder()
            .label("Install")
            .css_classes(["suggested-action"])
            .halign(Align::End)
            .build();

        let footer_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .margin_top(6)
            .build();
        footer_box.append(&progress_revealer);
        footer_box.append(&status_revealer);
        footer_box.append(&action_button);

        let clamp = adw::Clamp::builder()
            .maximum_size(520)
            .tightening_threshold(380)
            .build();
        let content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(10)
            .build();
        content.append(&identity_group);
        content.append(&status_group);
        content.append(&details_group);
        content.append(&footer_box);
        clamp.set_child(Some(&content));

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&clamp));

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&toolbar_view));
        window.set_content(Some(&toast_overlay));

        Self {
            window,
            package_name_label,
            version_label,
            arch_label,
            path_label,
            state_row,
            context_label,
            status_revealer,
            status_page,
            progress_revealer,
            spinner,
            progress,
            action_button,
            toast_overlay,
            detail_rows,
            details_expander,
        }
    }

    pub fn bind_package(
        &self,
        info: &RpmInfo,
        installed: &InstalledState,
        action_mode: ActionMode,
    ) {
        self.package_name_label.set_label(&info.name);
        self.version_label.set_label(&format!(
            "Version: {}{}-{}",
            info.epoch.map(|e| format!("{e}:")).unwrap_or_default(),
            info.version,
            info.release
        ));
        self.arch_label
            .set_label(&format!("Architecture: {}", info.arch));
        self.path_label
            .set_label(&format!("Path: {}", info.path.display()));

        let (subtitle, state_text) = match installed.relation {
            InstallRelation::NotInstalled => {
                ("Package is not currently installed.", "Not installed")
            }
            InstallRelation::SameVersion => (
                "Same version is installed; reinstall is available.",
                "Same version installed",
            ),
            InstallRelation::Upgrade => (
                "An older version is installed; this performs an upgrade.",
                "Upgrade available",
            ),
            InstallRelation::Downgrade => (
                "A newer version is installed; installing will downgrade.",
                "Downgrade",
            ),
        };

        self.state_row.set_subtitle(subtitle);
        self.context_label.set_label(
            &installed
                .installed_evr_arch
                .clone()
                .unwrap_or_else(|| state_text.to_string()),
        );

        let button_label = match action_mode {
            ActionMode::Install | ActionMode::Downgrade => BackendOperation::Install.label(),
            ActionMode::Reinstall => BackendOperation::Reinstall.label(),
        };
        self.action_button.set_label(button_label);

        let values = [
            info.summary.clone().unwrap_or_else(|| "—".to_string()),
            info.description.clone().unwrap_or_else(|| "—".to_string()),
            info.license.clone().unwrap_or_else(|| "—".to_string()),
            info.vendor.clone().unwrap_or_else(|| "—".to_string()),
            info.packager.clone().unwrap_or_else(|| "—".to_string()),
            info.url.clone().unwrap_or_else(|| "—".to_string()),
            format_size(info.installed_size),
            format_size(info.package_size),
            info.source_rpm.clone().unwrap_or_else(|| "—".to_string()),
            info.signature_status
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
        ];

        for (idx, row) in self.detail_rows.borrow().iter().enumerate() {
            if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
                label.set_label(values.get(idx).map(String::as_str).unwrap_or("—"));
            }
        }
    }

    pub fn set_running(&self, running: bool) {
        self.action_button.set_sensitive(!running);
        self.progress_revealer.set_reveal_child(running);
        self.spinner.set_spinning(running);
        if running {
            self.progress.set_fraction(0.0);
            self.progress.set_text(Some("Working…"));
        } else {
            self.progress.set_fraction(0.0);
            self.progress.set_text(None);
        }
    }

    pub fn set_progress(&self, progress_percent: Option<u32>) {
        if let Some(pct) = progress_percent {
            self.progress.set_fraction((pct.min(100) as f64) / 100.0);
            self.progress.set_text(Some(&format!("{pct}%")));
        } else {
            self.progress.pulse();
            self.progress.set_text(Some("Working…"));
        }
    }

    pub fn show_status(&self, icon: &str, title: &str, body: &str, css: Option<&str>) {
        self.status_page.set_icon_name(Some(icon));
        self.status_page.set_title(title);
        self.status_page.set_description(Some(body));
        self.status_page.remove_css_class("error");
        self.status_page.remove_css_class("success");
        if let Some(css_class) = css {
            self.status_page.add_css_class(css_class);
        }
        self.status_page.set_visible(true);
        self.status_revealer.set_reveal_child(true);
    }
}

    pub fn hide_status(&self) {
        self.status_page.set_visible(false);
        self.status_revealer.set_reveal_child(false);
    }

    rows
}

fn shorten_middle(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len || max_len <= 5 {
        return input.to_string();
    }
}

    let keep = (max_len - 1) / 2;
    let start = input.chars().take(keep).collect::<String>();
    let end = input
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    format!("{start}…{end}")
}

fn insert_if_text(map: &mut HashMap<DetailKey, String>, key: DetailKey, value: Option<&str>) {
    if let Some(text) = value.map(str::trim).filter(|v| !v.is_empty()) {
        map.insert(key, text.to_string());
    }
}

fn insert_if_u64<F>(
    map: &mut HashMap<DetailKey, String>,
    key: DetailKey,
    value: Option<u64>,
    formatter: F,
) where
    F: Fn(Option<u64>) -> String,
{
    if let Some(v) = value {
        map.insert(key, formatter(Some(v)));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn shortens_middle_path_for_hero_label() {
        let long =
            "/very/long/path/to/some/rpm/files/build-output/package-name-1.2.3-1.fc42.x86_64.rpm";
        let short = shorten_middle(long, 32);
        assert!(short.contains('…'));
        assert!(short.len() <= 33);
    }

    #[test]
    fn metadata_model_skips_empty_fields() {
        let info = RpmInfo {
            path: PathBuf::from("/tmp/test.rpm"),
            name: "testpkg".to_string(),
            epoch: None,
            version: "1.2.3".to_string(),
            release: "1.fc42".to_string(),
            arch: "x86_64".to_string(),
            summary: Some("".to_string()),
            description: Some("Useful package".to_string()),
            license: None,
            vendor: None,
            packager: None,
            url: Some("https://example.org".to_string()),
            installed_size: None,
            package_size: Some(1024),
            source_rpm: None,
            signature_status: Some("Signed".to_string()),
        };

        let installed = InstalledState {
            relation: InstallRelation::NotInstalled,
            installed_evr_arch: None,
        };

        let model = PackageViewModel::from_inputs(&info, &installed, ActionMode::Install);

        assert!(!model.details.contains_key(&DetailKey::Summary));
        assert!(!model.details.contains_key(&DetailKey::InstalledSize));
        assert_eq!(
            model
                .details
                .get(&DetailKey::Description)
                .map(String::as_str),
            Some("Useful package")
        );
        assert_eq!(
            model.details.get(&DetailKey::Homepage).map(String::as_str),
            Some("https://example.org")
        );
        assert_eq!(
            model.details.get(&DetailKey::Signature).map(String::as_str),
            Some("Signed")
        );
    }
}

fn secondary_label() -> gtk::Label {
    gtk::Label::builder()
        .halign(Align::Start)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build()
}
