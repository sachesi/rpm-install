use crate::backend::types::{BackendOperation, TransactionPreview};
use crate::error::{AppError, AppResult};
use tracing::{info, warn};

pub async fn preview_local_rpm_transaction(
    spec: &str,
    operation: BackendOperation,
) -> AppResult<TransactionPreview> {
    let mut cmd = vec!["zypper", "--non-interactive", "--no-gpg-checks"];
    match operation {
        BackendOperation::Install | BackendOperation::Reinstall | BackendOperation::Upgrade | BackendOperation::Downgrade => {
            cmd.push("install");
            cmd.push("--dry-run");
        }
        BackendOperation::Remove => {
            cmd.push("remove");
            cmd.push("--dry-run");
        }
    }
    cmd.push(spec);

    info!("Running zypper preview: {:?}", cmd);

    let cmd_os: Vec<&std::ffi::OsStr> = cmd.iter().map(std::ffi::OsStr::new).collect();
    let proc = gio::Subprocess::newv(&cmd_os, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE)
        .map_err(|e| AppError::Other(e.into()))?;

    let (stdout, stderr) = proc.communicate_utf8_future(None).await
        .map_err(|e| AppError::Other(e.into()))?;

    if !proc.has_exited() || proc.exit_status() != 0 {
        let err_msg = stderr.unwrap_or_default();
        warn!("Zypper preview failed: {}", err_msg);
    }

    let stdout = stdout.unwrap_or_default();
    Ok(parse_zypper_preview(&stdout))
}

pub async fn run_local_rpm_transaction<F>(
    spec: &str,
    operation: BackendOperation,
    mut on_progress: F,
) -> AppResult<BackendOperation>
where
    F: FnMut(Option<u32>) + 'static,
{
    let mut cmd = vec!["pkexec", "zypper", "--non-interactive", "--no-gpg-checks"];
    match operation {
        BackendOperation::Install | BackendOperation::Reinstall | BackendOperation::Upgrade | BackendOperation::Downgrade => {
            cmd.push("install");
        }
        BackendOperation::Remove => {
            cmd.push("remove");
        }
    }
    cmd.push(spec);

    info!("Running zypper transaction: {:?}", cmd);
    on_progress(None);

    let cmd_os: Vec<&std::ffi::OsStr> = cmd.iter().map(std::ffi::OsStr::new).collect();
    let proc = gio::Subprocess::newv(&cmd_os, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE)
        .map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("dismissed") || msg.contains("canceled") {
                AppError::AuthCanceled
            } else if msg.contains("not authorized") {
                AppError::AuthDenied
            } else {
                AppError::Other(e.into())
            }
        })?;

    let (_stdout, stderr) = proc.communicate_utf8_future(None).await
        .map_err(|e| AppError::Other(e.into()))?;

    if !proc.has_exited() || proc.exit_status() != 0 {
        let err_msg = stderr.unwrap_or_default();
        if err_msg.contains("Access denied") || err_msg.contains("not authorized") {
             return Err(AppError::AuthDenied);
        }
        return Err(AppError::OperationFailed {
            operation,
            details: format!("Zypper failed: {}", err_msg),
        });
    }

    on_progress(Some(100));
    Ok(operation)
}

fn parse_zypper_preview(stdout: &str) -> TransactionPreview {
    let mut additional_package_changes = Vec::new();
    let mut capturing = false;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        if line.contains("The following") && (line.contains("NEW") || line.contains("updated") || line.contains("removed")) {
            capturing = true;
            continue;
        }

        if capturing {
            if line.contains(":") && !line.contains("  ") {
                capturing = false;
                continue;
            }
            
            // Split by whitespace and add to list if it looks like a package name
            for pkg in line.split_whitespace() {
                if !pkg.is_empty() && !pkg.starts_with('(') && !pkg.ends_with(')') {
                    additional_package_changes.push(pkg.to_string());
                }
            }
        }
    }

    additional_package_changes.sort();
    additional_package_changes.dedup();

    TransactionPreview {
        additional_package_changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zypper_preview_output() {
        let output = r#"
The following 3 NEW packages are going to be installed:
  libfoo1 libbar2 baz

The following package is going to be REMOVED:
  oldpkg

3 new packages to install, 1 to remove.
"#;
        let preview = parse_zypper_preview(output);
        assert_eq!(preview.additional_package_changes.len(), 4);
        assert!(preview.additional_package_changes.contains(&"libfoo1".to_string()));
        assert!(preview.additional_package_changes.contains(&"oldpkg".to_string()));
    }
}
