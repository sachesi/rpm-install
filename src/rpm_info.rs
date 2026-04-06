use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use rpm::Package;

use crate::error::{AppError, AppResult};

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
    pub fn nevra(&self) -> String {
        match self.epoch {
            Some(epoch) => format!(
                "{}-{}:{}-{}.{}",
                self.name, epoch, self.version, self.release, self.arch
            ),
            None => format!(
                "{}-{}-{}.{}",
                self.name, self.version, self.release, self.arch
            ),
        }
    }

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
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let metadata = fs::metadata(&canonical)
        .with_context(|| format!("Could not read metadata for {}", canonical.display()))
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    if metadata.is_dir() {
        return Err(AppError::DirectoryNotSupported);
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
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let package_size = fs::metadata(path).ok().map(|m| m.len());
    let signature_status = match pkg.signature_key_ids() {
        Ok(ids) if !ids.is_empty() => Some(format!("Signed (key IDs: {})", ids.join(", "))),
        Ok(_) => Some("Signed".to_string()),
        Err(_) => Some("No verifiable signature metadata".to_string()),
    };

    let info = RpmInfo {
        path: path.to_path_buf(),
        name: pkg.metadata.get_name().unwrap_or("unknown").to_string(),
        epoch: pkg.metadata.get_epoch().ok().filter(|epoch| *epoch != 0),
        version: pkg.metadata.get_version().unwrap_or("unknown").to_string(),
        release: pkg.metadata.get_release().unwrap_or("unknown").to_string(),
        arch: pkg.metadata.get_arch().unwrap_or("unknown").to_string(),
        summary: pkg.metadata.get_summary().ok().map(ToString::to_string),
        description: pkg.metadata.get_description().ok().map(ToString::to_string),
        license: pkg.metadata.get_license().ok().map(ToString::to_string),
        vendor: pkg.metadata.get_vendor().ok().map(ToString::to_string),
        packager: pkg.metadata.get_packager().ok().map(ToString::to_string),
        url: pkg.metadata.get_url().ok().map(ToString::to_string),
        installed_size: pkg.metadata.get_installed_size().ok(),
        package_size,
        source_rpm: pkg.metadata.get_source_rpm().ok().map(ToString::to_string),
        signature_status,
    };

    if info.is_source_rpm() {
        return Err(AppError::SourceRpmNotInstallable);
    }

    Ok(info)
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
