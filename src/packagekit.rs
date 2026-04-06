use anyhow::Context;
use futures_util::StreamExt;
use packagekit_zbus::package_kit::PackageKitProxy;
use packagekit_zbus::transaction::TransactionProxy;
use tracing::{info, warn};

use crate::error::{AppError, AppResult};

const PK_FLAG_NONE: u64 = 0;
const PK_FLAG_ALLOW_REINSTALL: u64 = 1 << 4;
const PK_FLAG_JUST_REINSTALL: u64 = 1 << 5;
const PK_FLAG_ALLOW_DOWNGRADE: u64 = 1 << 6;

const PK_EXIT_SUCCESS: u32 = 1;
const PK_EXIT_CANCELLED: u32 = 3;
const PK_EXIT_CANCELLED_PRIORITY: u32 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMode {
    Install,
    Reinstall,
    Downgrade,
}

pub async fn install_local_file<F>(
    file_path: &str,
    package_name: &str,
    target_arch: &str,
    target_evr: &str,
    mode: InstallMode,
    mut on_progress: F,
) -> AppResult<InstallMode>
where
    F: FnMut(u32) + 'static,
{
    let connection = zbus::Connection::system()
        .await
        .context("Could not connect to system D-Bus")
        .map_err(AppError::Other)?;

    let flags = match mode {
        InstallMode::Install => PK_FLAG_NONE,
        InstallMode::Reinstall => PK_FLAG_ALLOW_REINSTALL | PK_FLAG_JUST_REINSTALL,
        InstallMode::Downgrade => PK_FLAG_ALLOW_DOWNGRADE,
    };

    let path_label = match mode {
        InstallMode::Install => "install",
        InstallMode::Reinstall => "reinstall-direct",
        InstallMode::Downgrade => "downgrade",
    };
    info!("packagekit-path={path_label}");

    match run_install_transaction(&connection, file_path, flags, &mut on_progress).await {
        Ok(()) => return Ok(mode),
        Err(AppError::PackageKit { code, details }) if mode == InstallMode::Reinstall => {
            if !is_reinstall_capability_error(code, &details) {
                return Err(AppError::PackageKit { code, details });
            }
            warn!("Direct reinstall unsupported by backend, falling back to remove+install");
        }
        Err(AppError::PackageKit { code, details }) if mode == InstallMode::Downgrade => {
            if is_downgrade_not_supported(code, &details) {
                return Err(AppError::DowngradeNotSupported);
            }
            return Err(AppError::PackageKit { code, details });
        }
        Err(err) => return Err(err),
    }

    info!("packagekit-path=reinstall-fallback");
    let package_id =
        resolve_installed_package_id(&connection, package_name, target_arch, target_evr)
            .await?
            .ok_or(AppError::ReinstallNotSupported)?;

    run_remove_transaction(&connection, &package_id).await?;
    run_install_transaction(&connection, file_path, PK_FLAG_NONE, &mut on_progress).await?;

    Ok(InstallMode::Reinstall)
}

async fn run_install_transaction<F>(
    connection: &zbus::Connection,
    file_path: &str,
    flags: u64,
    on_progress: &mut F,
) -> AppResult<()>
where
    F: FnMut(u32) + 'static,
{
    let tx = create_transaction(connection).await?;

    let mut progress_stream = tx
        .receive_item_progress()
        .await
        .context("Could not subscribe to PackageKit item progress")
        .map_err(AppError::Other)?;
    let mut error_stream = tx
        .receive_error_code()
        .await
        .context("Could not subscribe to PackageKit errors")
        .map_err(AppError::Other)?;
    let mut finished_stream = tx
        .receive_finished()
        .await
        .context("Could not subscribe to PackageKit completion")
        .map_err(AppError::Other)?;

    tx.install_files(flags, &[file_path])
        .await
        .context("PackageKit install call failed")
        .map_err(AppError::Other)?;

    let mut completion_exit: Option<u32> = None;
    loop {
        futures_util::select! {
            progress = progress_stream.next() => {
                if let Some(signal) = progress {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid item progress signal")))?;
                    on_progress(*args.percentage());
                }
            }
            error = error_stream.next() => {
                if let Some(signal) = error {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid error signal")))?;
                    let details = args.details().to_string();
                    if is_cancellation_code(*args.code()) || is_cancellation_details(&details) {
                        return Err(AppError::InstallationCanceled);
                    }
                    return Err(AppError::PackageKit { code: *args.code(), details });
                }
            }
            finished = finished_stream.next() => {
                if let Some(signal) = finished {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid finished signal")))?;
                    completion_exit = Some(*args.exit());
                    break;
                }
            }
            complete => break,
        }
    }

    map_transaction_exit(completion_exit, "install")
}

async fn run_remove_transaction(connection: &zbus::Connection, package_id: &str) -> AppResult<()> {
    let tx = create_transaction(connection).await?;

    let mut error_stream = tx
        .receive_error_code()
        .await
        .context("Could not subscribe to PackageKit remove errors")
        .map_err(AppError::Other)?;
    let mut finished_stream = tx
        .receive_finished()
        .await
        .context("Could not subscribe to PackageKit remove completion")
        .map_err(AppError::Other)?;

    tx.remove_packages(PK_FLAG_NONE, &[package_id], false, false)
        .await
        .context("PackageKit remove call failed")
        .map_err(AppError::Other)?;

    let mut completion_exit: Option<u32> = None;
    loop {
        futures_util::select! {
            error = error_stream.next() => {
                if let Some(signal) = error {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid remove error signal")))?;
                    let details = args.details().to_string();
                    if is_cancellation_code(*args.code()) || is_cancellation_details(&details) {
                        return Err(AppError::InstallationCanceled);
                    }
                    return Err(AppError::PackageKit { code: *args.code(), details });
                }
            }
            finished = finished_stream.next() => {
                if let Some(signal) = finished {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid remove finished signal")))?;
                    completion_exit = Some(*args.exit());
                    break;
                }
            }
            complete => break,
        }
    }

    map_transaction_exit(completion_exit, "remove")
}

fn map_transaction_exit(exit: Option<u32>, phase: &str) -> AppResult<()> {
    match exit {
        Some(PK_EXIT_SUCCESS) => Ok(()),
        Some(PK_EXIT_CANCELLED | PK_EXIT_CANCELLED_PRIORITY) => Err(AppError::InstallationCanceled),
        Some(code) => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit {phase} transaction ended unsuccessfully (exit code {code})"
        ))),
        None => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit {phase} transaction did not report completion"
        ))),
    }
}

async fn resolve_installed_package_id(
    connection: &zbus::Connection,
    package_name: &str,
    target_arch: &str,
    target_evr: &str,
) -> AppResult<Option<String>> {
    let tx = create_transaction(connection).await?;

    let mut package_stream = tx
        .receive_package()
        .await
        .context("Could not subscribe to PackageKit package resolve signals")
        .map_err(AppError::Other)?;
    let mut error_stream = tx
        .receive_error_code()
        .await
        .context("Could not subscribe to PackageKit resolve errors")
        .map_err(AppError::Other)?;
    let mut finished_stream = tx
        .receive_finished()
        .await
        .context("Could not subscribe to PackageKit resolve completion")
        .map_err(AppError::Other)?;

    tx.resolve(0, &[package_name])
        .await
        .context("PackageKit resolve call failed")
        .map_err(AppError::Other)?;

    let mut exact_match: Option<String> = None;
    loop {
        futures_util::select! {
            package = package_stream.next() => {
                if let Some(signal) = package {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid resolve package signal")))?;
                    let package_id = args.package_id().to_string();
                    if is_exact_package_identity_match(&package_id, package_name, target_arch, target_evr) {
                        exact_match = Some(package_id);
                    }
                }
            }
            error = error_stream.next() => {
                if let Some(signal) = error {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid resolve error signal")))?;
                    let details = args.details().to_string();
                    if is_cancellation_code(*args.code()) || is_cancellation_details(&details) {
                        return Err(AppError::InstallationCanceled);
                    }
                    return Err(AppError::PackageKit { code: *args.code(), details });
                }
            }
            finished = finished_stream.next() => {
                if finished.is_some() {
                    break;
                }
            }
            complete => break,
        }
    }

    Ok(exact_match)
}

fn is_exact_package_identity_match(
    package_id: &str,
    name: &str,
    arch: &str,
    target_evr: &str,
) -> bool {
    let mut parts = package_id.split(';');
    let Some(pid_name) = parts.next() else {
        return false;
    };
    let Some(pid_version) = parts.next() else {
        return false;
    };
    let Some(pid_arch) = parts.next() else {
        return false;
    };

    pid_name == name && pid_arch == arch && pid_version == target_evr
}

async fn create_transaction(connection: &zbus::Connection) -> AppResult<TransactionProxy<'_>> {
    let pk = PackageKitProxy::new(connection)
        .await
        .context("Could not create PackageKit proxy")
        .map_err(AppError::Other)?;

    let tx_path = pk
        .create_transaction()
        .await
        .context("Could not create PackageKit transaction")
        .map_err(AppError::Other)?;

    TransactionProxy::builder(connection)
        .path(tx_path)
        .context("Invalid PackageKit transaction path")
        .map_err(AppError::Other)?
        .build()
        .await
        .context("Could not bind PackageKit transaction proxy")
        .map_err(AppError::Other)
}

fn is_reinstall_capability_error(code: u32, details: &str) -> bool {
    let detail_lc = details.to_lowercase();
    code == 9
        || detail_lc.contains("reinstall")
        || detail_lc.contains("already installed")
        || detail_lc.contains("not supported")
}

fn is_downgrade_not_supported(code: u32, details: &str) -> bool {
    let detail_lc = details.to_lowercase();
    code == 9 || detail_lc.contains("downgrade") || detail_lc.contains("not supported")
}

fn is_cancellation_code(code: u32) -> bool {
    code == PK_EXIT_CANCELLED || code == PK_EXIT_CANCELLED_PRIORITY
}

fn is_cancellation_details(details: &str) -> bool {
    let detail_lc = details.to_lowercase();
    detail_lc.contains("cancel") || detail_lc.contains("denied")
}
