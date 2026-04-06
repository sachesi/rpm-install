use anyhow::Context;
use futures_util::StreamExt;
use packagekit_zbus::package_kit::PackageKitProxy;
use packagekit_zbus::transaction::TransactionProxy;

use crate::error::{AppError, AppResult};

const PK_FLAG_NONE: u64 = 0;
const PK_FLAG_ALLOW_REINSTALL: u64 = 1 << 5;

#[derive(Clone, Copy, Debug)]
pub enum InstallMode {
    Install,
    Reinstall,
}

#[derive(Clone, Debug)]
pub struct InstallResult {
    pub reinstalled: bool,
}

pub async fn install_local_file<F>(
    file_path: &str,
    mode: InstallMode,
    mut on_progress: F,
) -> AppResult<InstallResult>
where
    F: FnMut(u32) + 'static,
{
    let connection = zbus::Connection::system()
        .await
        .context("Could not connect to system D-Bus")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

    let pk = PackageKitProxy::new(&connection)
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

    let tx = TransactionProxy::builder(&connection)
        .path(tx_path)
        .context("Invalid PackageKit transaction path")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?
        .build()
        .await
        .context("Could not bind PackageKit transaction proxy")
        .map_err(anyhow::Error::from)
        .map_err(AppError::Other)?;

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

    let mut flags = PK_FLAG_NONE;
    if matches!(mode, InstallMode::Reinstall) {
        flags |= PK_FLAG_ALLOW_REINSTALL;
    }

    if let Err(err) = tx.install_files(flags, &[file_path]).await {
        if matches!(mode, InstallMode::Reinstall) {
            tx.install_files(PK_FLAG_NONE, &[file_path])
                .await
                .context("Reinstall flag unsupported and fallback install failed")
                .map_err(anyhow::Error::from)
                .map_err(AppError::Other)?;
        } else {
            return Err(AppError::Other(
                anyhow::Error::new(err).context("PackageKit install call failed"),
            ));
        }
    }

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
        Some(1) | Some(5) => Ok(InstallResult {
            reinstalled: matches!(mode, InstallMode::Reinstall),
        }),
        Some(_) => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit transaction ended unsuccessfully"
        ))),
        None => Err(AppError::Other(anyhow::anyhow!(
            "PackageKit transaction did not report completion"
        ))),
    }
}
