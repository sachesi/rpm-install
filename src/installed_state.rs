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
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    if !output.status.success() {
        return Ok(InstalledState {
            relation: crate::state_logic::InstallRelation::NotInstalled,
            installed_evr_arch: None,
        });
    }

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

    let installed: Vec<PackageIdentity> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('|').collect();
            if fields.len() != 5 {
                return None;
            }
            Some(PackageIdentity {
                name: fields[0].trim().to_string(),
                evr: format!(
                    "{}:{}-{}",
                    fields[1].trim(),
                    fields[2].trim(),
                    fields[3].trim()
                ),
                arch: fields[4].trim().to_string(),
            })
        })
        .collect();

    let ClassifiedState {
        relation,
        installed_evr_arch,
    } = classify_state(&local, &installed);

    Ok(InstalledState {
        relation,
        installed_evr_arch,
    })
}
