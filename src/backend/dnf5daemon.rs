use std::collections::HashMap;

use anyhow::Context;
use futures_util::{FutureExt, StreamExt};
use tracing::{info, warn};
use zbus::{Connection, Proxy};
use zvariant::{OwnedObjectPath, Value};

use crate::backend::types::BackendOperation;
use crate::error::{AppError, AppResult};

const BUS_NAME: &str = "org.rpm.dnf.v0";
const ROOT_PATH: &str = "/org/rpm/dnf/v0";
const IFACE_SESSION_MANAGER: &str = "org.rpm.dnf.v0.SessionManager";
const IFACE_RPM: &str = "org.rpm.dnf.v0.rpm.Rpm";
const IFACE_GOAL: &str = "org.rpm.dnf.v0.Goal";
const IFACE_BASE: &str = "org.rpm.dnf.v0.Base";

const RESOLVE_OK: u32 = 0;
const RESOLVE_WARNINGS: u32 = 1;

pub async fn run_local_rpm_transaction<F>(
    rpm_path: &str,
    operation: BackendOperation,
    mut on_progress: F,
) -> AppResult<BackendOperation>
where
    F: FnMut(Option<u32>) + 'static,
{
    let connection = Connection::system().await.map_err(map_connect_error)?;

    let session_manager = proxy(&connection, ROOT_PATH, IFACE_SESSION_MANAGER).await?;
    let session_path = open_session(&session_manager).await?;
    info!(%session_path, ?operation, "Opened dnf5daemon session");

    let op_result = run_in_session(
        &connection,
        &session_path,
        rpm_path,
        operation,
        &mut on_progress,
    )
    .await;

    if let Err(close_err) = close_session(&session_manager, &session_path).await {
        warn!(
            "Failed closing dnf5daemon session {}: {close_err}",
            session_path.as_str()
        );
    }

    op_result.map(|_| operation)
}

async fn run_in_session<F>(
    connection: &Connection,
    session_path: &OwnedObjectPath,
    rpm_path: &str,
    operation: BackendOperation,
    on_progress: &mut F,
) -> AppResult<()>
where
    F: FnMut(Option<u32>) + 'static,
{
    let rpm = proxy(connection, session_path.as_str(), IFACE_RPM).await?;
    let goal = proxy(connection, session_path.as_str(), IFACE_GOAL).await?;
    let base = proxy(connection, session_path.as_str(), IFACE_BASE).await?;

    let specs = vec![rpm_path.to_string()];
    let empty_options = HashMap::<String, Value<'_>>::new();

    call_rpm_op(&rpm, operation, &(specs, empty_options)).await?;

    let resolve_options = HashMap::from([("allow_erasing".to_string(), Value::from(true))]);
    let (_, resolve_result): (
        Vec<(
            String,
            String,
            String,
            HashMap<String, Value<'_>>,
            HashMap<String, Value<'_>>,
        )>,
        u32,
    ) = goal
        .call("resolve", &(resolve_options,))
        .await
        .map_err(|err| map_dbus_error(err, operation))?;

    if resolve_result != RESOLVE_OK && resolve_result != RESOLVE_WARNINGS {
        let problems: Vec<String> = goal
            .call("get_transaction_problems_string", &())
            .await
            .unwrap_or_else(|_| vec!["Dependency resolution failed.".to_string()]);
        return Err(AppError::OperationFailed {
            operation,
            details: problems.join("\n"),
        });
    }

    on_progress(None);

    let mut download_stream = base
        .receive_signal("download_progress")
        .await
        .context("Could not subscribe to dnf5daemon download progress")
        .map_err(AppError::Other)?;
    let mut action_stream = rpm
        .receive_signal("transaction_action_progress")
        .await
        .context("Could not subscribe to dnf5daemon transaction progress")
        .map_err(AppError::Other)?;

    let tx_options = HashMap::from([("interactive".to_string(), Value::from(true))]);
    let mut run_tx = goal
        .call_method("do_transaction", &(tx_options,))
        .map(|r| r.map_err(|err| map_dbus_error(err, operation)))
        .fuse();

    loop {
        futures_util::select! {
            finished = run_tx => {
                finished?;
                break;
            }
            download = download_stream.next() => {
                if let Some(msg) = download {
                    if let Ok((signal_session, _, total, downloaded)) = msg.body().deserialize::<(OwnedObjectPath, String, i64, i64)>() {
                        if signal_session == *session_path && total > 0 {
                            let pct = ((downloaded.max(0) as f64 / total as f64) * 100.0).round() as u32;
                            on_progress(Some(pct.min(100)));
                        }
                    }
                }
            }
            action = action_stream.next() => {
                if let Some(msg) = action {
                    if let Ok((signal_session, _, processed, total)) = msg.body().deserialize::<(OwnedObjectPath, String, u64, u64)>() {
                        if signal_session == *session_path && total > 0 {
                            let pct = ((processed as f64 / total as f64) * 100.0).round() as u32;
                            on_progress(Some(pct.min(100)));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn proxy<'a>(
    connection: &'a Connection,
    path: &'a str,
    interface: &'a str,
) -> AppResult<Proxy<'a>> {
    Proxy::new(connection, BUS_NAME, path, interface)
        .await
        .with_context(|| format!("Could not create dnf5daemon proxy for {interface}"))
        .map_err(AppError::Other)
}

async fn open_session(session_manager: &Proxy<'_>) -> AppResult<OwnedObjectPath> {
    let options = HashMap::<String, Value<'_>>::new();
    session_manager
        .call("open_session", &(options,))
        .await
        .map_err(|err| map_dbus_error(err, BackendOperation::Install))
}

async fn close_session(
    session_manager: &Proxy<'_>,
    session_path: &OwnedObjectPath,
) -> AppResult<()> {
    let _closed: bool = session_manager
        .call("close_session", &(session_path,))
        .await
        .context("Failed to close dnf5daemon session")
        .map_err(AppError::Other)?;
    Ok(())
}

async fn call_rpm_op(
    rpm: &Proxy<'_>,
    operation: BackendOperation,
    payload: &(Vec<String>, HashMap<String, Value<'_>>),
) -> AppResult<()> {
    let method = match operation {
        BackendOperation::Install => "install",
        BackendOperation::Reinstall => "reinstall",
        BackendOperation::Upgrade => "upgrade",
        BackendOperation::Downgrade => "downgrade",
    };

    rpm.call_method(method, payload)
        .await
        .map_err(|err| map_dbus_error(err, operation))?;
    Ok(())
}

fn map_connect_error(err: zbus::Error) -> AppError {
    match err {
        zbus::Error::InputOutput(io_err) => AppError::DaemonUnavailable(io_err.to_string()),
        other => {
            AppError::Other(anyhow::Error::new(other).context("Could not connect to system D-Bus"))
        }
    }
}

fn map_dbus_error(err: zbus::Error, operation: BackendOperation) -> AppError {
    let msg = err.to_string();
    let msg_lc = msg.to_lowercase();

    if msg.contains("org.freedesktop.DBus.Error.ServiceUnknown")
        || msg.contains("org.freedesktop.DBus.Error.NameHasNoOwner")
        || msg.contains("org.freedesktop.DBus.Error.Spawn.ServiceNotFound")
    {
        return AppError::DaemonUnavailable(msg);
    }

    if msg_lc.contains("not authorized") || msg_lc.contains("access denied") {
        return AppError::AuthDenied;
    }

    if msg_lc.contains("authorization") && msg_lc.contains("dismissed") {
        return AppError::AuthCanceled;
    }

    if msg_lc.contains("aborted") || msg_lc.contains("cancel") {
        return AppError::TransactionCanceled;
    }

    if msg_lc.contains("unsupported") || msg_lc.contains("not supported") {
        return AppError::UnsupportedOperation(operation);
    }

    if msg_lc.contains("cannot open")
        || msg_lc.contains("not a valid")
        || msg_lc.contains("no such file")
    {
        return AppError::InvalidLocalRpm(msg);
    }

    AppError::OperationFailed {
        operation,
        details: msg,
    }
}
