use std::process::Command;

use anyhow::Context;

use crate::error::{AppError, AppResult};
use crate::rpm_info::RpmInfo;

#[derive(Clone, Debug)]
pub struct InstalledState {
    pub installed: bool,
    pub installed_evr_arch: Option<String>,
}

pub fn detect_installed(info: &RpmInfo) -> AppResult<InstalledState> {
    let query = "%{EPOCHNUM}:%{VERSION}-%{RELEASE}.%{ARCH}";
    let output = Command::new("rpm")
        .arg("-q")
        .arg("--qf")
        .arg(query)
        .arg(&info.name)
        .output()
        .with_context(|| "Failed to invoke rpm query command")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    if !output.status.success() {
        return Ok(InstalledState {
            installed: false,
            installed_evr_arch: None,
        });
    }

    let current = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let local = format!(
        "{}:{}-{}.{}",
        info.epoch.unwrap_or(0),
        info.version,
        info.release,
        info.arch
    );

    Ok(InstalledState {
        installed: current == local,
        installed_evr_arch: Some(current),
    })
}
