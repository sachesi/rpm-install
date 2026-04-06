use std::collections::HashMap;

use adw::prelude::*;
use gtk::pango;
use gtk::{Align, Orientation};

use crate::backend::types::BackendOperation;
use crate::installed_state::InstalledState;
use crate::rpm_info::{RpmInfo, format_size};
use crate::state_logic::ActionMode;

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

const OUTER_MARGIN: i32 = 16;
const SECTION_SPACING: i32 = 12;
const CARD_PADDING: i32 = 14;
const FOOTER_PADDING: i32 = 12;

#[derive(Clone)]
struct DetailBinding {
    container: gtk::Box,
    value: gtk::Label,
}

#[derive(Clone)]
pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub package_name_label: gtk::Label,
    pub version_label: gtk::Label,
    pub path_label: gtk::Label,
    pub context_label: gtk::Label,
    pub status_revealer: gtk::Revealer,
    pub progress_revealer: gtk::Revealer,
    pub spinner: gtk::Spinner,
    pub progress: gtk::ProgressBar,
    pub action_button: gtk::Button,
    pub state_title_label: gtk::Label,
    state_subtitle_label: gtk::Label,
    status_icon: gtk::Image,
    status_title: gtk::Label,
    status_body: gtk::Label,
    details_title: gtk::Label,
    detail_rows: HashMap<DetailKey, DetailBinding>,
}

#[derive(Debug)]
struct PackageViewModel {
    package_name: String,
    version_arch: String,
    path_display: String,
    installed_state_title: String,
    installed_subtitle: String,
    installed_context: String,
    action_label: &'static str,
    details: HashMap<DetailKey, String>,
}

impl PackageViewModel {
    fn from_inputs(info: &RpmInfo, installed: &InstalledState, action_mode: ActionMode) -> Self {
        let summary = installed.relation.summary();
        let (installed_state_title, installed_subtitle, fallback_state) = (
            summary.state_title,
            summary.state_subtitle,
            summary.fallback_context.to_string(),
        );

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

        Self {
            package_name: info.name.clone(),
            version_arch: format!(
                "{epoch_prefix}{}-{} • {}",
                info.version, info.release, info.arch
            ),
            path_display: shorten_middle(&info.path.display().to_string(), 72),
            installed_state_title: installed_state_title.to_string(),
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
        header.add_css_class("flat");
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
        version_label.set_margin_top(2);

        let path_label = gtk::Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["caption", "dim-label"])
            .selectable(true)
            .build();
        path_label.set_ellipsize(pango::EllipsizeMode::Middle);
        path_label.set_margin_top(4);

        let hero_content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_start(CARD_PADDING)
            .margin_end(CARD_PADDING)
            .margin_top(CARD_PADDING)
            .margin_bottom(CARD_PADDING)
            .build();
        hero_content.append(&package_name_label);
        hero_content.append(&version_label);
        hero_content.append(&path_label);

        let hero_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .build();
        hero_box.add_css_class("card");
        hero_box.append(&hero_content);

        let state_title_label = gtk::Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        let state_subtitle_label = secondary_label();
        let context_label = gtk::Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .css_classes(["monospace", "dim-label"])
            .wrap(true)
            .build();
        context_label.set_margin_top(4);

        let state_content = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_start(CARD_PADDING)
            .margin_end(CARD_PADDING)
            .margin_top(CARD_PADDING)
            .margin_bottom(CARD_PADDING)
            .build();
        state_content.append(&state_title_label);
        state_content.append(&state_subtitle_label);
        state_content.append(&context_label);

        let state_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .build();
        state_box.add_css_class("card");
        state_box.append(&state_content);

        let details_chevron = gtk::Image::from_icon_name("pan-end-symbolic");
        let details_title = gtk::Label::builder()
            .label("Package details")
            .halign(Align::Start)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let details_toggle_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .margin_start(CARD_PADDING)
            .margin_end(CARD_PADDING)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        details_toggle_box.append(&details_title);
        details_toggle_box.append(&details_chevron);

        let details_toggle = gtk::ToggleButton::builder().build();
        details_toggle.set_child(Some(&details_toggle_box));
        details_toggle.add_css_class("flat");

        let details_list = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .margin_start(CARD_PADDING)
            .margin_end(CARD_PADDING)
            .margin_top(CARD_PADDING / 2)
            .margin_bottom(CARD_PADDING)
            .build();

        let details_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        details_revealer.set_child(Some(&details_list));

        let details_card = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();
        details_card.add_css_class("card");
        details_card.append(&details_toggle);
        details_card.append(&details_revealer);

        let mut detail_rows = HashMap::new();
        for (key, title) in DETAIL_ORDER {
            let key_label = gtk::Label::builder()
                .label(title)
                .halign(Align::Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            let value_label = gtk::Label::builder()
                .halign(Align::Start)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .selectable(true)
                .build();

            let row = gtk::Box::builder()
                .orientation(Orientation::Vertical)
                .spacing(2)
                .build();
            row.append(&key_label);
            row.append(&value_label);

            details_list.append(&row);
            detail_rows.insert(
                key,
                DetailBinding {
                    container: row,
                    value: value_label,
                },
            );
        }

        let details_chevron_for_toggle = details_chevron.clone();
        let details_revealer_for_toggle = details_revealer.clone();
        details_toggle.connect_toggled(move |toggle| {
            let expanded = toggle.is_active();
            details_revealer_for_toggle.set_reveal_child(expanded);
            details_chevron_for_toggle.set_icon_name(Some(if expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        });

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
            .margin_start(FOOTER_PADDING)
            .margin_end(FOOTER_PADDING)
            .margin_top(8)
            .margin_bottom(8)
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
            .margin_start(FOOTER_PADDING)
            .margin_end(FOOTER_PADDING)
            .margin_top(10)
            .margin_bottom(4)
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
            .spacing(8)
            .margin_start(FOOTER_PADDING)
            .margin_end(FOOTER_PADDING)
            .margin_top(8)
            .margin_bottom(FOOTER_PADDING)
            .build();
        let footer_spacer = gtk::Box::builder().hexpand(true).build();
        footer_actions.append(&footer_spacer);
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
            .spacing(SECTION_SPACING)
            .margin_start(OUTER_MARGIN)
            .margin_end(OUTER_MARGIN)
            .margin_top(OUTER_MARGIN)
            .margin_bottom(OUTER_MARGIN)
            .build();
        content.append(&hero_box);
        content.append(&state_box);
        content.append(&details_card);

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
            context_label,
            status_revealer,
            progress_revealer,
            spinner,
            progress,
            action_button,
            state_title_label,
            state_subtitle_label,
            status_icon,
            status_title,
            status_body,
            details_title,
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
        self.state_title_label
            .set_label(&model.installed_state_title);
        self.state_subtitle_label
            .set_label(&model.installed_subtitle);
        self.context_label.set_label(&model.installed_context);
        self.action_button.set_label(model.action_label);
        self.details_title.set_label(if model.details.is_empty() {
            "No package details"
        } else {
            "Package details"
        });

        for (key, _) in DETAIL_ORDER {
            if let Some(binding) = self.detail_rows.get(&key) {
                if let Some(value) = model.details.get(&key) {
                    binding.value.set_label(value);
                    binding.container.set_visible(true);
                } else {
                    binding.value.set_label("");
                    binding.container.set_visible(false);
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
