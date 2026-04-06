use std::cmp::Ordering;
use std::process::Command;

use anyhow::Context;
use rpm::rpm_evr_compare;

use crate::error::{AppError, AppResult};
use crate::rpm_info::RpmInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallRelation {
    NotInstalled,
    SameVersion,
    Upgrade,
    Downgrade,
}

#[derive(Clone, Debug)]
pub struct InstalledState {
    pub relation: InstallRelation,
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
            relation: InstallRelation::NotInstalled,
            installed_evr_arch: None,
        });
    }

    let local_evr = format!(
        "{}:{}-{}",
        info.epoch.unwrap_or(0),
        info.version,
        info.release
    );
    let local_arch = info.arch.as_str();

    let mut selected: Option<(String, String, String)> = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != 5 {
            continue;
        }

        let installed_arch = fields[4].trim().to_string();
        let arch_matches =
            installed_arch == local_arch || local_arch == "noarch" || installed_arch == "noarch";
        if !arch_matches {
            continue;
        }

        let evr = format!(
            "{}:{}-{}",
            fields[1].trim(),
            fields[2].trim(),
            fields[3].trim()
        );
        selected = Some((evr, installed_arch.clone(), line.to_string()));

        if installed_arch == local_arch {
            break;
        }
    }

    let Some((installed_evr, installed_arch, _raw)) = selected else {
        return Ok(InstalledState {
            relation: InstallRelation::NotInstalled,
            installed_evr_arch: None,
        });
    };

    let relation = match rpm_evr_compare(&local_evr, &installed_evr) {
        Ordering::Equal => InstallRelation::SameVersion,
        Ordering::Greater => InstallRelation::Upgrade,
        Ordering::Less => InstallRelation::Downgrade,
    };

    Ok(InstalledState {
        relation,
        installed_evr_arch: Some(format!("{installed_evr}.{installed_arch}")),
    })
}
