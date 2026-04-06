use std::collections::HashMap;

use adw::prelude::*;
use gtk::pango;
use gtk::{Align, Orientation};

use crate::backend::types::BackendOperation;
use crate::installed_state::InstalledState;
use crate::rpm_info::{RpmInfo, format_size};
use crate::state_logic::{ActionMode, InstallRelation};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DetailKey {
    Summary,
    Description,
    License,
    Vendor,
    Packager,
    Homepage,
    InstalledSize,
    PackageSize,
    SourceRpm,
    Signature,
}

const DETAIL_ORDER: [(DetailKey, &str); 10] = [
    (DetailKey::Summary, "Summary"),
    (DetailKey::Description, "Description"),
    (DetailKey::License, "License"),
    (DetailKey::Vendor, "Vendor"),
    (DetailKey::Packager, "Packager"),
    (DetailKey::Homepage, "Homepage"),
    (DetailKey::InstalledSize, "Installed size"),
    (DetailKey::PackageSize, "Package size"),
    (DetailKey::SourceRpm, "Source RPM"),
    (DetailKey::Signature, "Signature"),
];

#[derive(Clone)]
struct DetailBinding {
    row: adw::ActionRow,
    value: gtk::Label,
}

#[derive(Clone)]
pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub package_name_label: gtk::Label,
    pub version_label: gtk::Label,
    pub path_label: gtk::Label,
    pub state_row: adw::ActionRow,
    pub context_label: gtk::Label,
    pub status_revealer: gtk::Revealer,
    pub progress_revealer: gtk::Revealer,
    pub spinner: gtk::Spinner,
    pub progress: gtk::ProgressBar,
    pub action_button: gtk::Button,
    status_icon: gtk::Image,
    status_title: gtk::Label,
    status_body: gtk::Label,
    detail_rows: HashMap<DetailKey, DetailBinding>,
}

#[derive(Debug)]
struct PackageViewModel {
    package_name: String,
    version_arch: String,
    path_display: String,
    installed_subtitle: String,
    installed_context: String,
    action_label: &'static str,
    details: HashMap<DetailKey, String>,
}

impl PackageViewModel {
    fn from_inputs(info: &RpmInfo, installed: &InstalledState, action_mode: ActionMode) -> Self {
        let (installed_subtitle, fallback_state) = match installed.relation {
            InstallRelation::NotInstalled => (
                "Not currently installed on this system.",
                "Not installed".to_string(),
            ),
            InstallRelation::SameVersion => (
                "Same version is installed. You can reinstall if needed.",
                "Same version installed".to_string(),
            ),
            InstallRelation::Upgrade => (
                "An older version is installed. This will upgrade it.",
                "Upgrade available".to_string(),
            ),
            InstallRelation::Downgrade => (
                "A newer version is installed. Installing this RPM will downgrade it.",
                "Downgrade".to_string(),
            ),
        };

        let action_label = match action_mode {
            ActionMode::Install | ActionMode::Downgrade => BackendOperation::Install.label(),
            ActionMode::Reinstall => BackendOperation::Reinstall.label(),
        };

        let mut details = HashMap::new();
        insert_if_text(&mut details, DetailKey::Summary, info.summary.as_deref());
        insert_if_text(
            &mut details,
            DetailKey::Description,
            info.description.as_deref(),
        );
        insert_if_text(&mut details, DetailKey::License, info.license.as_deref());
        insert_if_text(&mut details, DetailKey::Vendor, info.vendor.as_deref());
        insert_if_text(&mut details, DetailKey::Packager, info.packager.as_deref());
        insert_if_text(&mut details, DetailKey::Homepage, info.url.as_deref());
        insert_if_u64(
            &mut details,
            DetailKey::InstalledSize,
            info.installed_size,
            format_size,
        );
        insert_if_u64(
            &mut details,
            DetailKey::PackageSize,
            info.package_size,
            format_size,
        );
        insert_if_text(
            &mut details,
            DetailKey::SourceRpm,
            info.source_rpm.as_deref(),
        );
        insert_if_text(
            &mut details,
            DetailKey::Signature,
            info.signature_status.as_deref(),
        );

        let epoch_prefix = info.epoch.map(|e| format!("{e}:")).unwrap_or_default();
        let path_display = shorten_middle(&info.path.display().to_string(), 72);

        Self {
            package_name: info.name.clone(),
            version_arch: format!(
                "{epoch_prefix}{}-{} • {}",
                info.version, info.release, info.arch
            ),
            path_display,
            installed_subtitle: installed_subtitle.to_string(),
            installed_context: installed
                .installed_evr_arch
                .clone()
                .unwrap_or(fallback_state),
            action_label,
            details,
        }
    }
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
            .title("Install Local RPM")
            .subtitle("Fedora")
            .build();
        header.set_title_widget(Some(&header_title));

        let package_name_label = gtk::Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["title-2"])
            .build();
        let version_label = secondary_label();

        let path_label = secondary_label();
        path_label.set_selectable(true);
        path_label.set_ellipsize(pango::EllipsizeMode::Middle);

        let hero_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_start(18)
            .margin_end(18)
            .margin_top(16)
            .margin_bottom(14)
            .build();
        hero_box.add_css_class("card");
        hero_box.append(&package_name_label);
        hero_box.append(&version_label);
        hero_box.append(&path_label);

        let state_row = adw::ActionRow::builder().title("Installed state").build();
        state_row.set_activatable(false);
        let context_label = gtk::Label::builder()
            .halign(Align::End)
            .xalign(1.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        state_row.add_suffix(&context_label);

        let state_group = adw::PreferencesGroup::builder().build();
        state_group.add(&state_row);

        let details_group = adw::PreferencesGroup::builder().title("Details").build();
        let details_expander = adw::ExpanderRow::builder()
            .title("Package metadata")
            .subtitle("Show additional package fields")
            .build();
        details_group.add(&details_expander);

        let mut detail_rows = HashMap::new();
        for (key, title) in DETAIL_ORDER {
            let row = adw::ActionRow::builder().title(title).build();
            row.set_activatable(false);

            let value = gtk::Label::builder()
                .halign(Align::End)
                .xalign(1.0)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .max_width_chars(48)
                .selectable(true)
                .build();
            value.set_ellipsize(pango::EllipsizeMode::End);

            row.add_suffix(&value);
            details_expander.add_row(&row);

            detail_rows.insert(key, DetailBinding { row, value });
        }

        let status_icon = gtk::Image::new();
        status_icon.set_icon_size(gtk::IconSize::Normal);
        let status_title = gtk::Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();
        let status_body = secondary_label();
        status_body.set_wrap_mode(pango::WrapMode::WordChar);

        let status_text_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();
        status_text_box.append(&status_title);
        status_text_box.append(&status_body);

        let status_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .margin_start(12)
            .margin_end(12)
            .margin_top(10)
            .margin_bottom(10)
            .build();
        status_box.add_css_class("card");
        status_box.append(&status_icon);
        status_box.append(&status_text_box);

        let status_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        status_revealer.set_child(Some(&status_box));

        let spinner = gtk::Spinner::builder().spinning(false).build();
        let progress = gtk::ProgressBar::builder()
            .hexpand(true)
            .show_text(true)
            .build();

        let progress_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .margin_start(12)
            .margin_end(12)
            .margin_top(10)
            .margin_bottom(2)
            .build();
        progress_box.append(&spinner);
        progress_box.append(&progress);

        let progress_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        progress_revealer.set_child(Some(&progress_box));

        let action_button = gtk::Button::builder()
            .label("Install")
            .css_classes(["suggested-action", "pill"])
            .halign(Align::End)
            .build();

        let footer_actions = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(10)
            .build();
        footer_actions.append(&action_button);

        let footer_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();
        footer_box.add_css_class("toolbar");
        footer_box.append(&progress_revealer);
        footer_box.append(&status_revealer);
        footer_box.append(&footer_actions);

        let content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        content.append(&hero_box);
        content.append(&state_group);
        content.append(&details_group);

        let clamp = adw::Clamp::builder()
            .maximum_size(520)
            .tightening_threshold(420)
            .build();
        clamp.set_child(Some(&content));

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&clamp));
        toolbar_view.add_bottom_bar(&footer_box);

        window.set_content(Some(&toolbar_view));

        Self {
            window,
            package_name_label,
            version_label,
            path_label,
            state_row,
            context_label,
            status_revealer,
            progress_revealer,
            spinner,
            progress,
            action_button,
            status_icon,
            status_title,
            status_body,
            detail_rows,
        }
    }

    pub fn bind_package(
        &self,
        info: &RpmInfo,
        installed: &InstalledState,
        action_mode: ActionMode,
    ) {
        let model = PackageViewModel::from_inputs(info, installed, action_mode);

        self.package_name_label.set_label(&model.package_name);
        self.version_label.set_label(&model.version_arch);
        self.path_label.set_label(&model.path_display);
        self.state_row.set_subtitle(&model.installed_subtitle);
        self.context_label.set_label(&model.installed_context);
        self.action_button.set_label(model.action_label);

        for (key, _) in DETAIL_ORDER {
            if let Some(binding) = self.detail_rows.get(&key) {
                if let Some(value) = model.details.get(&key) {
                    binding.value.set_label(value);
                    binding.row.set_visible(true);
                } else {
                    binding.row.set_visible(false);
                    binding.value.set_label("");
                }
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
        if let Some(percent) = progress_percent {
            self.progress
                .set_fraction(f64::from(percent.min(100)) / 100.0);
            self.progress.set_text(Some(&format!("{percent}%")));
        } else {
            self.progress.pulse();
            self.progress.set_text(Some("Working…"));
        }
    }

    pub fn show_status(&self, icon: &str, title: &str, body: &str, css: Option<&str>) {
        self.status_icon.set_icon_name(Some(icon));
        self.status_title.set_label(title);
        self.status_body.set_label(body);

        for class_name in ["error", "success"] {
            self.status_title.remove_css_class(class_name);
            self.status_body.remove_css_class(class_name);
        }

        if let Some(class_name) = css {
            self.status_title.add_css_class(class_name);
            self.status_body.add_css_class(class_name);
        }

        self.status_revealer.set_reveal_child(true);
    }

    pub fn hide_status(&self) {
        self.status_revealer.set_reveal_child(false);
        self.status_title.set_label("");
        self.status_body.set_label("");
    }
}

fn shorten_middle(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len || max_len <= 5 {
        return input.to_string();
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

fn secondary_label() -> gtk::Label {
    gtk::Label::builder()
        .halign(Align::Start)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build()
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
