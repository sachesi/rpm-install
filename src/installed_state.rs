use std::process::Command;

use anyhow::Context;

use crate::error::{AppError, AppResult};
use crate::rpm_info::RpmInfo;
use crate::state_logic::{ClassifiedState, PackageIdentity, classify_state};

#[derive(Clone, Debug)]
pub struct InstalledState {
    pub relation: crate::state_logic::InstallRelation,
    pub installed_evr_arch: Option<String>,
}

pub fn detect_installed(info: &RpmInfo) -> AppResult<InstalledState> {
    let query = "%{NAME}|%{EPOCHNUM}|%{VERSION}|%{RELEASE}|%{ARCH}\n";
    let output = Command::new("rpm")
        .arg("-q")
        .arg("--qf")
        .arg(query)
        .arg(&info.name)
        .output()
        .with_context(|| "Failed to invoke rpm query command")
        .map_err(AppError::Other)?;

    if !output.status.success() {
        if output.status.code() == Some(1) {
            return Ok(InstalledState {
                relation: crate::state_logic::InstallRelation::NotInstalled,
                installed_evr_arch: None,
            });
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Other(anyhow::anyhow!(
            "rpm query failed with status {:?}: {}",
            output.status.code(),
            stderr
        )));
    }

    let stdout = std::str::from_utf8(&output.stdout)
        .context("rpm output was not valid UTF-8")
        .map_err(AppError::Other)?;

    let local = PackageIdentity {
        name: info.name.clone(),
        evr: format!(
            "{}:{}-{}",
            info.epoch.unwrap_or(0),
            info.version,
            info.release
        ),
        arch: info.arch.clone(),
    };

    let installed: Vec<PackageIdentity> = stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '|').map(str::trim);
            let name = fields.next()?;
            let epoch = fields.next()?;
            let version = fields.next()?;
            let release = fields.next()?;
            let arch = fields.next()?;
            Some(PackageIdentity {
                name: name.to_string(),
                evr: format!("{epoch}:{version}-{release}"),
                arch: arch.to_string(),
            })
        })
        .collect();

    if installed.is_empty() {
        return Ok(InstalledState {
            relation: crate::state_logic::InstallRelation::NotInstalled,
            installed_evr_arch: None,
        });
    }

    let ClassifiedState {
        relation,
        installed_evr_arch,
    } = classify_state(&local, &installed);

    Ok(InstalledState {
        relation,
        installed_evr_arch,
    })
}
