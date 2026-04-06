use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use gio::prelude::FileExt;
use rpm::Package;

use crate::error::{AppError, AppResult};
const MAX_FIELD_CHARS: usize = 4_096;

#[derive(Clone, Debug)]
pub struct RpmInfo {
    pub path: PathBuf,
    pub name: String,
    pub epoch: Option<u32>,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub vendor: Option<String>,
    pub packager: Option<String>,
    pub url: Option<String>,
    pub installed_size: Option<u64>,
    pub package_size: Option<u64>,
    pub source_rpm: Option<String>,
    pub signature_status: Option<String>,
}

impl RpmInfo {
    pub fn is_source_rpm(&self) -> bool {
        self.arch == "src" || self.arch == "nosrc"
    }
}

pub fn canonicalize_and_validate(input: &str) -> AppResult<PathBuf> {
    if input.starts_with("file://") {
        let file = gio::File::for_uri(input);
        if !file.is_native() {
            return Err(AppError::NonLocalPath(input.to_string()));
        }
        if let Some(path) = file.path() {
            return validate_local_path(&path);
        }
        return Err(AppError::NonLocalPath(input.to_string()));
    }

    validate_local_path(Path::new(input))
}

fn validate_local_path(path: &Path) -> AppResult<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("Could not resolve path {}", path.display()))
        .map_err(AppError::Other)?;

    let metadata = fs::metadata(&canonical)
        .with_context(|| format!("Could not read metadata for {}", canonical.display()))
        .map_err(AppError::Other)?;

    if metadata.is_dir() {
        return Err(AppError::DirectoryNotSupported);
    }
    if !metadata.file_type().is_file() {
        return Err(AppError::NonRegularFile);
    }

    let is_rpm = canonical
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("rpm"))
        .unwrap_or(false);

    if !is_rpm {
        return Err(AppError::UnsupportedFileType);
    }

    Ok(canonical)
}

pub fn read_rpm_info(path: &Path) -> AppResult<RpmInfo> {
    let pkg = Package::open(path)
        .with_context(|| format!("Failed to parse RPM file {}", path.display()))
        .map_err(AppError::Other)?;

    let package_size = fs::metadata(path).ok().map(|m| m.len());
    let signature_status = match pkg.signature_key_ids() {
        Ok(ids) if !ids.is_empty() => {
            Some(clamp_text(&format!("Signed (key IDs: {})", ids.join(", "))))
        }
        Ok(_) => Some("Signed".to_string()),
        Err(_) => Some("No verifiable signature metadata".to_string()),
    };

    let info = RpmInfo {
        path: path.to_path_buf(),
        name: clamp_text(pkg.metadata.get_name().unwrap_or("unknown")),
        epoch: pkg.metadata.get_epoch().ok().filter(|epoch| *epoch != 0),
        version: clamp_text(pkg.metadata.get_version().unwrap_or("unknown")),
        release: clamp_text(pkg.metadata.get_release().unwrap_or("unknown")),
        arch: clamp_text(pkg.metadata.get_arch().unwrap_or("unknown")),
        summary: pkg.metadata.get_summary().ok().map(clamp_text),
        description: pkg.metadata.get_description().ok().map(clamp_text),
        license: pkg.metadata.get_license().ok().map(clamp_text),
        vendor: pkg.metadata.get_vendor().ok().map(clamp_text),
        packager: pkg.metadata.get_packager().ok().map(clamp_text),
        url: pkg.metadata.get_url().ok().map(clamp_text),
        installed_size: pkg.metadata.get_installed_size().ok(),
        package_size,
        source_rpm: pkg.metadata.get_source_rpm().ok().map(clamp_text),
        signature_status,
    };

    if info.is_source_rpm() {
        return Err(AppError::SourceRpmNotInstallable);
    }

    Ok(info)
}

fn clamp_text(input: &str) -> String {
    let mut out = input.chars().take(MAX_FIELD_CHARS).collect::<String>();
    if input.chars().count() > MAX_FIELD_CHARS {
        out.push('…');
    }
    out
}

pub fn format_size(bytes: Option<u64>) -> String {
    match bytes {
        Some(size) => {
            const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
            let mut value = size as f64;
            let mut unit = 0usize;
            while value >= 1024.0 && unit < UNITS.len() - 1 {
                value /= 1024.0;
                unit += 1;
            }
            format!("{value:.1} {}", UNITS[unit])
        }
        None => "Unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};

    #[test]
    fn clamp_text_limits_large_values() {
        let huge = "a".repeat(MAX_FIELD_CHARS + 50);
        let clamped = clamp_text(&huge);
        assert_eq!(clamped.chars().count(), MAX_FIELD_CHARS + 1);
        assert!(clamped.ends_with('…'));
    }

    #[test]
    fn rejects_directory_input() {
        let dir = std::env::temp_dir();
        let err = validate_local_path(&dir).expect_err("directory must be rejected");
        assert!(matches!(err, AppError::DirectoryNotSupported));
    }

    #[test]
    fn accepts_regular_rpm_file() {
        let file_path =
            std::env::temp_dir().join(format!("rpm-installer-test-{}.rpm", std::process::id()));
        let _file = File::create(&file_path).expect("create temp rpm");
        let validated = validate_local_path(&file_path).expect("must accept rpm extension");
        assert!(validated.ends_with(file_path.file_name().expect("file name exists")));
        fs::remove_file(file_path).expect("cleanup temp file");
    }
}
