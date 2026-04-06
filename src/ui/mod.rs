use std::collections::HashMap;

use adw::prelude::*;
use gtk::pango;
use gtk::{Align, Orientation};

use crate::backend::types::BackendOperation;
use crate::installed_state::InstalledState;
use crate::rpm_info::{RpmInfo, format_size};
use crate::state_logic::{ActionMode, InstallRelation};

const HERO_PATH_MAX_CHARS: usize = 68;

#[derive(Clone)]
pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub package_name_label: gtk::Label,
    pub subtitle_label: gtk::Label,
    pub path_label: gtk::Label,
    pub state_row: adw::ActionRow,
    pub context_label: gtk::Label,
    pub status_revealer: gtk::Revealer,
    pub status_icon: gtk::Image,
    pub status_title_label: gtk::Label,
    pub status_body_label: gtk::Label,
    pub progress_revealer: gtk::Revealer,
    pub spinner: gtk::Spinner,
    pub progress: gtk::ProgressBar,
    pub action_button: gtk::Button,
    pub toast_overlay: adw::ToastOverlay,
    detail_rows: Vec<DetailRowBinding>,
    details_expander: adw::ExpanderRow,
}

#[derive(Clone)]
struct DetailRowBinding {
    key: DetailKey,
    row: adw::ActionRow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageViewModel {
    package_name: String,
    subtitle: String,
    path: String,
    state_title: String,
    state_subtitle: String,
    installed_context: String,
    action_label: String,
    details: HashMap<DetailKey, String>,
}

impl PackageViewModel {
    fn from_inputs(info: &RpmInfo, installed: &InstalledState, action_mode: ActionMode) -> Self {
        let state_title = match installed.relation {
            InstallRelation::NotInstalled => "Not installed",
            InstallRelation::SameVersion => "Same version installed",
            InstallRelation::Upgrade => "Upgrade available",
            InstallRelation::Downgrade => "Downgrade warning",
        }
        .to_string();

        let state_subtitle = match installed.relation {
            InstallRelation::NotInstalled => "This package is not currently installed.",
            InstallRelation::SameVersion => {
                "This exact build is installed; reinstall is available."
            }
            InstallRelation::Upgrade => {
                "An older installed version was found. Installing this RPM upgrades it."
            }
            InstallRelation::Downgrade => {
                "A newer installed version was found. Installing this RPM downgrades it."
            }
        }
        .to_string();

        let installed_context = installed
            .installed_evr_arch
            .as_ref()
            .map(|v| format!("Installed: {v}"))
            .unwrap_or_else(|| "Installed: not present".to_string());

        let action_label = match action_mode {
            ActionMode::Install | ActionMode::Downgrade => BackendOperation::Install.label(),
            ActionMode::Reinstall => BackendOperation::Reinstall.label(),
        }
        .to_string();

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

        Self {
            package_name: info.name.clone(),
            subtitle: format!(
                "{}{}-{} · {}",
                info.epoch.map(|e| format!("{e}:")).unwrap_or_default(),
                info.version,
                info.release,
                info.arch
            ),
            path: shorten_middle(&info.path.display().to_string(), HERO_PATH_MAX_CHARS),
            state_title,
            state_subtitle,
            installed_context,
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
            .title("Local RPM Installer")
            .subtitle("Fedora")
            .build();
        header.set_title_widget(Some(&header_title));

        let package_name_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .css_classes(["title-1"])
            .build();
        let subtitle_label = gtk::Label::builder()
            .halign(Align::Start)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let path_label = gtk::Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .wrap(false)
            .ellipsize(pango::EllipsizeMode::Middle)
            .css_classes(["caption", "dim-label"])
            .build();

        let hero_block = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        hero_block.add_css_class("card");
        hero_block.append(&package_name_label);
        hero_block.append(&subtitle_label);
        hero_block.append(&path_label);

        let state_row = adw::ActionRow::builder().build();
        let context_label = gtk::Label::builder()
            .xalign(1.0)
            .wrap(true)
            .css_classes(["caption", "dim-label"])
            .build();
        state_row.add_suffix(&context_label);
        state_row.set_activatable(false);

        let state_group = adw::PreferencesGroup::builder()
            .title("Installed state")
            .build();
        state_group.add(&state_row);

        let details_expander = adw::ExpanderRow::builder()
            .title("Package details")
            .subtitle("No additional metadata")
            .build();

        let detail_rows = make_detail_rows(&details_expander);

        let details_group = adw::PreferencesGroup::builder().title("Details").build();
        details_group.add(&details_expander);

        let content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(12)
            .margin_end(12)
            .build();
        content.append(&hero_block);
        content.append(&state_group);
        content.append(&details_group);

        let clamp = adw::Clamp::builder()
            .maximum_size(520)
            .tightening_threshold(380)
            .child(&content)
            .build();

        let status_icon = gtk::Image::from_icon_name("dialog-information-symbolic");
        let status_title_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();
        let status_body_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let status_text = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .build();
        status_text.append(&status_title_label);
        status_text.append(&status_body_label);

        let status_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .margin_bottom(2)
            .build();
        status_box.append(&status_icon);
        status_box.append(&status_text);

        let status_revealer = gtk::Revealer::builder().reveal_child(false).build();
        status_revealer.set_child(Some(&status_box));

        let spinner = gtk::Spinner::builder().spinning(false).build();
        let progress = gtk::ProgressBar::builder()
            .hexpand(true)
            .show_text(true)
            .build();
        let progress_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .margin_top(6)
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
            .spacing(4)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(10)
            .build();
        footer_box.append(&progress_revealer);
        footer_box.append(&status_revealer);
        footer_box.append(&action_button);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&clamp));
        toolbar_view.add_bottom_bar(&footer_box);

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&toolbar_view));
        window.set_content(Some(&toast_overlay));

        Self {
            window,
            package_name_label,
            subtitle_label,
            path_label,
            state_row,
            context_label,
            status_revealer,
            status_icon,
            status_title_label,
            status_body_label,
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
        let model = PackageViewModel::from_inputs(info, installed, action_mode);

        self.package_name_label.set_label(&model.package_name);
        self.subtitle_label.set_label(&model.subtitle);
        self.path_label.set_label(&model.path);

        self.state_row.set_title(&model.state_title);
        self.state_row.set_subtitle(&model.state_subtitle);
        self.context_label.set_label(&model.installed_context);
        self.action_button.set_label(&model.action_label);

        let mut shown = 0usize;
        for binding in &self.detail_rows {
            if let Some(value) = model.details.get(&binding.key) {
                binding.row.set_subtitle(value);
                binding.row.set_visible(true);
                shown += 1;
            } else {
                binding.row.set_subtitle("");
                binding.row.set_visible(false);
            }
        }

        self.details_expander.set_subtitle(match shown {
            0 => "No additional metadata",
            1 => "1 metadata field",
            _ => "Metadata fields available",
        });
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
        self.status_icon.set_icon_name(Some(icon));
        self.status_title_label.set_label(title);
        self.status_body_label.set_label(body);
        self.status_title_label.remove_css_class("error");
        self.status_title_label.remove_css_class("success");
        if let Some(class_name) = css {
            self.status_title_label.add_css_class(class_name);
        }
        self.status_revealer.set_reveal_child(true);
    }

    pub fn hide_status(&self) {
        self.status_revealer.set_reveal_child(false);
    }

    pub fn toast(&self, message: &str) {
        self.toast_overlay.add_toast(adw::Toast::new(message));
    }
}

fn make_detail_rows(expander: &adw::ExpanderRow) -> Vec<DetailRowBinding> {
    let mut rows = Vec::new();

    for (title, key) in [
        ("Summary", DetailKey::Summary),
        ("Description", DetailKey::Description),
        ("License", DetailKey::License),
        ("Vendor", DetailKey::Vendor),
        ("Packager", DetailKey::Packager),
        ("Homepage", DetailKey::Homepage),
        ("Installed size", DetailKey::InstalledSize),
        ("Package size", DetailKey::PackageSize),
        ("Source RPM", DetailKey::SourceRpm),
        ("Signature", DetailKey::Signature),
    ] {
        let row = adw::ActionRow::builder().title(title).build();
        row.set_activatable(false);
        row.set_visible(false);
        expander.add_row(&row);
        rows.push(DetailRowBinding { key, row });
    }

    rows
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
