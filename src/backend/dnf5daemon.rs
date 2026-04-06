use std::collections::HashMap;

use anyhow::Context;
use futures_util::{FutureExt, StreamExt};
use tracing::{info, warn};
use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;
use zbus::zvariant::OwnedValue;

use crate::backend::types::BackendOperation;
use crate::error::{AppError, AppResult};

const DNF5_BUS: &str = "org.rpm.dnf.v0";
const DNF5_ROOT_PATH: &str = "/org/rpm/dnf/v0";

#[proxy(
    interface = "org.rpm.dnf.v0.SessionManager",
    default_service = "org.rpm.dnf.v0",
    default_path = "/org/rpm/dnf/v0"
)]
trait SessionManager {
    fn open_session(&self, options: HashMap<&str, OwnedValue>) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.rpm.dnf.v0.rpm.Rpm",
    default_service = "org.rpm.dnf.v0"
)]
trait Rpm {
    fn install(&self, pkg_specs: Vec<&str>, options: HashMap<&str, OwnedValue>)
    -> zbus::Result<()>;
    fn upgrade(&self, pkg_specs: Vec<&str>, options: HashMap<&str, OwnedValue>)
    -> zbus::Result<()>;
    fn downgrade(
        &self,
        pkg_specs: Vec<&str>,
        options: HashMap<&str, OwnedValue>,
    ) -> zbus::Result<()>;
    fn reinstall(
        &self,
        pkg_specs: Vec<&str>,
        options: HashMap<&str, OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn transaction_action_progress(
        &self,
        session_object_path: OwnedObjectPath,
        nevra: &str,
        processed: u64,
        total: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn transaction_transaction_progress(
        &self,
        session_object_path: OwnedObjectPath,
        processed: u64,
        total: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn transaction_after_complete(
        &self,
        session_object_path: OwnedObjectPath,
        success: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn transaction_unpack_error(
        &self,
        session_object_path: OwnedObjectPath,
        nevra: &str,
    ) -> zbus::Result<()>;
}

#[proxy(interface = "org.rpm.dnf.v0.Goal", default_service = "org.rpm.dnf.v0")]
trait Goal {
    fn resolve(&self, options: HashMap<&str, OwnedValue>) -> zbus::Result<(Vec<OwnedValue>, u32)>;
    fn do_transaction(&self, options: HashMap<&str, OwnedValue>) -> zbus::Result<()>;
    fn get_transaction_problems_string(&self) -> zbus::Result<Vec<String>>;
}

pub async fn install_local_file<F>(
    file_path: &str,
    operation: BackendOperation,
    mut on_progress: F,
) -> AppResult<BackendOperation>
where
    F: FnMut(u32) + 'static,
{
    let connection = zbus::Connection::system()
        .await
        .map_err(|err| map_connection_error(err, "Could not connect to dnf5daemon"))?;

    let session_proxy = SessionManagerProxy::builder(&connection)
        .destination(DNF5_BUS)
        .map_err(AppError::Other)?
        .path(DNF5_ROOT_PATH)
        .map_err(AppError::Other)?
        .build()
        .await
        .map_err(|err| map_connection_error(err, "Could not reach dnf5daemon SessionManager"))?;

    let session_path = session_proxy
        .open_session(HashMap::new())
        .await
        .map_err(|err| map_method_error(err, "Could not open dnf5daemon session"))?;

    let rpm_proxy = RpmProxy::builder(&connection)
        .path(session_path.clone())
        .map_err(AppError::Other)?
        .build()
        .await
        .map_err(|err| map_method_error(err, "Could not open dnf5daemon rpm interface"))?;

    let goal_proxy = GoalProxy::builder(&connection)
        .path(session_path.clone())
        .map_err(AppError::Other)?
        .build()
        .await
        .map_err(|err| map_method_error(err, "Could not open dnf5daemon goal interface"))?;

    info!("backend=dnf5daemon op={operation:?} target={file_path}");

    enqueue_operation(&rpm_proxy, file_path, operation).await?;

    let (_items, resolve_result) = goal_proxy
        .resolve(HashMap::new())
        .await
        .map_err(|err| map_method_error(err, "Failed to resolve dnf5 transaction"))?;

    if resolve_result != 0 {
        let problems = goal_proxy
            .get_transaction_problems_string()
            .await
            .unwrap_or_default();
        let details = if problems.is_empty() {
            format!("Resolver returned result code {resolve_result}")
        } else {
            problems.join("; ")
        };
        return Err(AppError::InstallFailure(details));
    }

    execute_transaction(&goal_proxy, &rpm_proxy, &session_path, &mut on_progress).await?;

    Ok(operation)
}

async fn enqueue_operation(
    rpm_proxy: &RpmProxy<'_>,
    file_path: &str,
    operation: BackendOperation,
) -> AppResult<()> {
    let pkg_specs = vec![file_path];
    let options = HashMap::new();

    let result = match operation {
        BackendOperation::Install => rpm_proxy.install(pkg_specs, options).await,
        BackendOperation::Upgrade => rpm_proxy.upgrade(pkg_specs, options).await,
        BackendOperation::Reinstall => rpm_proxy.reinstall(pkg_specs, options).await,
        BackendOperation::Downgrade => rpm_proxy.downgrade(pkg_specs, options).await,
    };

    result.map_err(|err| map_method_error(err, "Failed to enqueue RPM operation"))
}

async fn execute_transaction<F>(
    goal_proxy: &GoalProxy<'_>,
    rpm_proxy: &RpmProxy<'_>,
    session_path: &OwnedObjectPath,
    on_progress: &mut F,
) -> AppResult<()>
where
    F: FnMut(u32),
{
    let mut action_progress = rpm_proxy
        .receive_transaction_action_progress()
        .await
        .context("Could not subscribe to dnf5daemon transaction action progress")
        .map_err(AppError::Other)?;
    let mut overall_progress = rpm_proxy
        .receive_transaction_transaction_progress()
        .await
        .context("Could not subscribe to dnf5daemon transaction preparation progress")
        .map_err(AppError::Other)?;
    let mut completion = rpm_proxy
        .receive_transaction_after_complete()
        .await
        .context("Could not subscribe to dnf5daemon completion signal")
        .map_err(AppError::Other)?;
    let mut unpack_error = rpm_proxy
        .receive_transaction_unpack_error()
        .await
        .context("Could not subscribe to dnf5daemon unpack-error signal")
        .map_err(AppError::Other)?;

    let mut tx_opts: HashMap<&str, OwnedValue> = HashMap::new();
    tx_opts.insert("offline", false.into());

    let mut do_tx = goal_proxy.do_transaction(tx_opts).fuse();
    let mut saw_completion = false;

    loop {
        futures_util::select! {
            result = do_tx => {
                result.map_err(|err| map_transaction_error(err, "Transaction execution failed"))?;
                break;
            }
            signal = action_progress.next() => {
                if let Some(msg) = signal {
                    let args = msg.args().map_err(AppError::Other)?;
                    if args.session_object_path() == session_path {
                        on_progress(progress_to_percent(*args.processed(), *args.total()));
                    }
                }
            }
            signal = overall_progress.next() => {
                if let Some(msg) = signal {
                    let args = msg.args().map_err(AppError::Other)?;
                    if args.session_object_path() == session_path {
                        on_progress(progress_to_percent(*args.processed(), *args.total()));
                    }
                }
            }
            signal = completion.next() => {
                if let Some(msg) = signal {
                    let args = msg.args().map_err(AppError::Other)?;
                    if args.session_object_path() == session_path {
                        saw_completion = true;
                        if !args.success() {
                            return Err(AppError::InstallFailure("dnf5daemon reported unsuccessful transaction completion".to_string()));
                        }
                    }
                }
            }
            signal = unpack_error.next() => {
                if let Some(msg) = signal {
                    let args = msg.args().map_err(AppError::Other)?;
                    if args.session_object_path() == session_path {
                        return Err(AppError::InstallFailure(format!(
                            "Failed while unpacking package {}",
                            args.nevra()
                        )));
                    }
                }
            }
            complete => break,
        }
    }

    if !saw_completion {
        warn!("dnf5daemon transaction completed without completion signal");
    }

    on_progress(100);
    Ok(())
}

fn progress_to_percent(processed: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }

    ((processed.saturating_mul(100)) / total).min(100) as u32
}

fn map_connection_error(err: zbus::Error, context: &str) -> AppError {
    if is_service_unavailable(&err) {
        return AppError::DaemonUnavailable(format!("{context}: {err}"));
    }

    AppError::Other(anyhow::Error::new(err).context(context.to_string()))
}

fn map_method_error(err: zbus::Error, context: &str) -> AppError {
    if is_service_unavailable(&err) {
        return AppError::DaemonUnavailable(err.to_string());
    }

    if is_auth_denied(&err) {
        return AppError::AuthCanceled;
    }

    if is_unsupported(&err) {
        return AppError::UnsupportedOperation(err.to_string());
    }

    AppError::Other(anyhow::Error::new(err).context(context.to_string()))
}

fn map_transaction_error(err: zbus::Error, context: &str) -> AppError {
    if is_auth_denied(&err) {
        return AppError::AuthCanceled;
    }
    if is_canceled(&err) {
        return AppError::TransactionCanceled;
    }

    if is_unsupported(&err) {
        return AppError::UnsupportedOperation(err.to_string());
    }

    AppError::InstallFailure(format!("{context}: {err}"))
}

fn is_service_unavailable(err: &zbus::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("serviceunknown")
        || msg.contains("namehasnoowner")
        || msg.contains("org.rpm.dnf.v0") && msg.contains("not provided")
}

fn is_auth_denied(err: &zbus::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("accessdenied")
        || msg.contains("authentication")
        || msg.contains("auth") && msg.contains("cancel")
        || msg.contains("polkit") && msg.contains("cancel")
}

fn is_canceled(err: &zbus::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("cancel") || msg.contains("aborted")
}

fn is_unsupported(err: &zbus::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("notsupported") || msg.contains("unsupported") || msg.contains("not implemented")
}
