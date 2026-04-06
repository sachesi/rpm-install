use anyhow::Context;
use futures_util::StreamExt;
use packagekit_zbus::package_kit::PackageKitProxy;
use packagekit_zbus::transaction::TransactionProxy;

use crate::error::{AppError, AppResult};

const PK_FLAG_NONE: u64 = 0;
const PK_FLAG_ALLOW_REINSTALL: u64 = 1 << 4;
const PK_FLAG_JUST_REINSTALL: u64 = 1 << 5;
const PK_FLAG_ALLOW_DOWNGRADE: u64 = 1 << 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMode {
    Install,
    Reinstall,
    Downgrade,
}

pub async fn install_local_file<F>(
    file_path: &str,
    package_name: &str,
    mode: InstallMode,
    mut on_progress: F,
) -> AppResult<InstallMode>
where
    F: FnMut(u32) + 'static,
{
    let connection = zbus::Connection::system()
        .await
        .context("Could not connect to system D-Bus")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let flags = match mode {
        InstallMode::Install => PK_FLAG_NONE,
        InstallMode::Reinstall => PK_FLAG_ALLOW_REINSTALL | PK_FLAG_JUST_REINSTALL,
        InstallMode::Downgrade => PK_FLAG_ALLOW_DOWNGRADE,
    };

    match run_install_transaction(&connection, file_path, flags, &mut on_progress).await {
        Ok(()) => return Ok(mode),
        Err(AppError::PackageKit { code, details }) if mode == InstallMode::Reinstall => {
            if !is_reinstall_capability_error(code, &details) {
                return Err(AppError::PackageKit { code, details });
            }
        }
        Err(err) => return Err(err),
    }

    let package_id = resolve_installed_package_id(&connection, package_name)
        .await?
        .ok_or_else(|| {
            AppError::Other(anyhow::anyhow!(
                "PackageKit backend did not provide direct reinstall and no installed package id could be resolved"
            ))
        })?;

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

    let mut percentage_stream = tx
        .receive_percentage()
        .await
        .context("Could not subscribe to PackageKit progress")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;
    let mut error_stream = tx
        .receive_error_code()
        .await
        .context("Could not subscribe to PackageKit errors")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;
    let mut finished_stream = tx
        .receive_finished()
        .await
        .context("Could not subscribe to PackageKit completion")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    tx.install_files(flags, &[file_path])
        .await
        .context("PackageKit install call failed")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let mut completion_exit: Option<u32> = None;
    loop {
        futures_util::select! {
            progress = percentage_stream.next() => {
                if let Some(signal) = progress {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid progress signal")))?;
                    on_progress(args.percentage());
                }
            }
            error = error_stream.next() => {
                if let Some(signal) = error {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid error signal")))?;
                    let details = args.details().to_string();
                    let detail_lc = details.to_lowercase();
                    if detail_lc.contains("cancel") || detail_lc.contains("denied") {
                        return Err(AppError::InstallationCanceled);
                    }
                    return Err(AppError::PackageKit { code: args.code(), details });
                }
            }
            finished = finished_stream.next() => {
                if let Some(signal) = finished {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid finished signal")))?;
                    completion_exit = Some(args.exit());
                    break;
                }
            }
            complete => break,
        }
    }

    match completion_exit {
        Some(1) | Some(5) => Ok(()),
        Some(_) => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit transaction ended unsuccessfully"
        ))),
        None => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit transaction did not report completion"
        ))),
    }
}

async fn run_remove_transaction(connection: &zbus::Connection, package_id: &str) -> AppResult<()> {
    let tx = create_transaction(connection).await?;

    let mut error_stream = tx
        .receive_error_code()
        .await
        .context("Could not subscribe to PackageKit remove errors")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;
    let mut finished_stream = tx
        .receive_finished()
        .await
        .context("Could not subscribe to PackageKit remove completion")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    tx.remove_packages(PK_FLAG_NONE, &[package_id], false, false)
        .await
        .context("PackageKit remove call failed")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let mut completion_exit: Option<u32> = None;
    loop {
        futures_util::select! {
            error = error_stream.next() => {
                if let Some(signal) = error {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid remove error signal")))?;
                    return Err(AppError::PackageKit { code: args.code(), details: args.details().to_string() });
                }
            }
            finished = finished_stream.next() => {
                if let Some(signal) = finished {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid remove finished signal")))?;
                    completion_exit = Some(args.exit());
                    break;
                }
            }
            complete => break,
        }
    }

    match completion_exit {
        Some(1) | Some(5) => Ok(()),
        Some(_) => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit remove transaction ended unsuccessfully"
        ))),
        None => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit remove transaction did not report completion"
        ))),
    }
}

async fn resolve_installed_package_id(
    connection: &zbus::Connection,
    package_name: &str,
) -> AppResult<Option<String>> {
    let tx = create_transaction(connection).await?;

    let mut package_stream = tx
        .receive_package()
        .await
        .context("Could not subscribe to PackageKit package resolve signals")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;
    let mut error_stream = tx
        .receive_error_code()
        .await
        .context("Could not subscribe to PackageKit resolve errors")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;
    let mut finished_stream = tx
        .receive_finished()
        .await
        .context("Could not subscribe to PackageKit resolve completion")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    tx.resolve(0, &[package_name])
        .await
        .context("PackageKit resolve call failed")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let mut candidate: Option<String> = None;
    loop {
        futures_util::select! {
            package = package_stream.next() => {
                if let Some(signal) = package {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid resolve package signal")))?;
                    let package_id = args.package_id();
                    if package_id.starts_with(&format!("{};", package_name)) {
                        candidate = Some(package_id.to_string());
                    }
                }
            }
            error = error_stream.next() => {
                if let Some(signal) = error {
                    let args = signal.args().map_err(|e| AppError::Other(anyhow::Error::new(e).context("Invalid resolve error signal")))?;
                    return Err(AppError::PackageKit { code: args.code(), details: args.details().to_string() });
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

    Ok(candidate)
}

async fn create_transaction(connection: &zbus::Connection) -> AppResult<TransactionProxy<'_>> {
    let pk = PackageKitProxy::new(connection)
        .await
        .context("Could not create PackageKit proxy")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let tx_path = pk
        .create_transaction()
        .await
        .context("Could not create PackageKit transaction")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    TransactionProxy::builder(connection)
        .path(tx_path)
        .context("Invalid PackageKit transaction path")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?
        .build()
        .await
        .context("Could not bind PackageKit transaction proxy")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)
}

fn is_reinstall_capability_error(code: u32, details: &str) -> bool {
    let detail_lc = details.to_lowercase();
    code == 9 || detail_lc.contains("reinstall") || detail_lc.contains("already installed")
}
