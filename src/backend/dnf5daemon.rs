use std::collections::HashMap;

use anyhow::Context;
use futures_util::{FutureExt, StreamExt};
use tracing::{info, warn};
use zbus::{Connection, Proxy};
use zvariant::{OwnedObjectPath, OwnedValue, Value};

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionPreview {
    pub additional_package_changes: Vec<String>,
}

#[derive(Debug)]
struct ResolvedTransactionItem {
    object_type: String,
    action: String,
    reason: String,
    object: HashMap<String, OwnedValue>,
}

pub async fn preview_local_rpm_transaction(
    spec: &str,
    operation: BackendOperation,
) -> AppResult<TransactionPreview> {
    let connection = Connection::system().await.map_err(map_connect_error)?;

    let session_manager = proxy(&connection, ROOT_PATH, IFACE_SESSION_MANAGER).await?;
    let session_path = open_session(&session_manager).await?;
    info!(%session_path, ?operation, "Opened dnf5daemon preview session");

    let preview_result = preview_in_session(&connection, &session_path, spec, operation).await;

    if let Err(close_err) = close_session(&session_manager, &session_path).await {
        warn!(
            "Failed closing dnf5daemon preview session {}: {close_err}",
            session_path.as_str()
        );
    }

    preview_result
}

pub async fn run_local_rpm_transaction<F>(
    spec: &str,
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
        spec,
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

async fn preview_in_session(
    connection: &Connection,
    session_path: &OwnedObjectPath,
    spec: &str,
    operation: BackendOperation,
) -> AppResult<TransactionPreview> {
    let rpm = proxy(connection, session_path.as_str(), IFACE_RPM).await?;
    let goal = proxy(connection, session_path.as_str(), IFACE_GOAL).await?;

    let specs = vec![spec.to_string()];
    let empty_options = HashMap::<String, Value<'_>>::new();

    call_rpm_op(&rpm, operation, &(specs, empty_options)).await?;

    let resolved_items = resolve_transaction(&goal, operation).await?;
    Ok(build_preview(&resolved_items))
}

async fn run_in_session<F>(
    connection: &Connection,
    session_path: &OwnedObjectPath,
    spec: &str,
    operation: BackendOperation,
    on_progress: &mut F,
) -> AppResult<()>
where
    F: FnMut(Option<u32>) + 'static,
{
    let rpm = proxy(connection, session_path.as_str(), IFACE_RPM).await?;
    let goal = proxy(connection, session_path.as_str(), IFACE_GOAL).await?;
    let base = proxy(connection, session_path.as_str(), IFACE_BASE).await?;

    let specs = vec![spec.to_string()];
    let empty_options = HashMap::<String, Value<'_>>::new();

    call_rpm_op(&rpm, operation, &(specs, empty_options)).await?;
    resolve_transaction(&goal, operation).await?;

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
    let tx_body = (tx_options,);
    let run_tx = goal
        .call_method("do_transaction", &tx_body)
        .map(|r| r.map_err(|err| map_dbus_error(err, operation)))
        .fuse();
    futures_util::pin_mut!(run_tx);

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

async fn resolve_transaction(
    goal: &Proxy<'_>,
    operation: BackendOperation,
) -> AppResult<Vec<ResolvedTransactionItem>> {
    let resolve_options = HashMap::from([("allow_erasing".to_string(), Value::from(true))]);
    let resolve_body = (resolve_options,);
    let (transaction_items, resolve_result): (
        Vec<(
            String,
            String,
            String,
            HashMap<String, OwnedValue>,
            HashMap<String, OwnedValue>,
        )>,
        u32,
    ) = goal
        .call("resolve", &resolve_body)
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

    Ok(transaction_items
        .into_iter()
        .map(
            |(object_type, action, reason, _attributes, object)| ResolvedTransactionItem {
                object_type,
                action,
                reason,
                object,
            },
        )
        .collect())
}

fn build_preview(items: &[ResolvedTransactionItem]) -> TransactionPreview {
    let mut additional_package_changes = items
        .iter()
        .filter(|item| is_additional_package_change(item))
        .filter_map(format_transaction_item)
        .collect::<Vec<_>>();
    additional_package_changes.sort();
    additional_package_changes.dedup();

    TransactionPreview {
        additional_package_changes,
    }
}

fn is_additional_package_change(item: &ResolvedTransactionItem) -> bool {
    item.object_type.eq_ignore_ascii_case("Package")
        && !item.reason.eq_ignore_ascii_case("User")
        && matches!(
            item.action.as_str(),
            "Install" | "Upgrade" | "Downgrade" | "Reinstall"
        )
}

fn format_transaction_item(item: &ResolvedTransactionItem) -> Option<String> {
    let subject = transaction_item_subject(&item.object)?;
    Some(format!("{} {}", item.action, subject))
}

fn transaction_item_subject(object: &HashMap<String, OwnedValue>) -> Option<String> {
    for key in ["full_nevra", "nevra"] {
        if let Some(value) = owned_string(object.get(key)) {
            return Some(value);
        }
    }

    let name = owned_string(object.get("name"))?;
    let evr = owned_string(object.get("evr"));
    let arch = owned_string(object.get("arch"));

    match (evr, arch) {
        (Some(evr), Some(arch)) => Some(format!("{name}-{evr}.{arch}")),
        (Some(evr), None) => Some(format!("{name}-{evr}")),
        _ => Some(name),
    }
}

fn owned_string(value: Option<&OwnedValue>) -> Option<String> {
    let value = value?;
    String::try_from(value.clone()).ok()
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
        BackendOperation::Remove => "remove",
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zvariant::{OwnedValue, Value};

    use super::{ResolvedTransactionItem, build_preview};

    fn owned_str(input: &str) -> OwnedValue {
        OwnedValue::try_from(Value::from(input)).expect("string value")
    }

    #[test]
    fn preview_lists_only_non_user_package_changes() {
        let local_user = ResolvedTransactionItem {
            object_type: "Package".to_string(),
            action: "Install".to_string(),
            reason: "User".to_string(),
            object: HashMap::from([("full_nevra".to_string(), owned_str("app-1:2.0-1.x86_64"))]),
        };
        let dependency = ResolvedTransactionItem {
            object_type: "Package".to_string(),
            action: "Install".to_string(),
            reason: "Dependency".to_string(),
            object: HashMap::from([(
                "full_nevra".to_string(),
                owned_str("libfoo-0:1.2.3-1.fc42.x86_64"),
            )]),
        };
        let upgrade = ResolvedTransactionItem {
            object_type: "Package".to_string(),
            action: "Upgrade".to_string(),
            reason: "Dependency".to_string(),
            object: HashMap::from([
                ("name".to_string(), owned_str("glibc")),
                ("evr".to_string(), owned_str("0:2.41-7.fc42")),
                ("arch".to_string(), owned_str("x86_64")),
            ]),
        };
        let group = ResolvedTransactionItem {
            object_type: "Group".to_string(),
            action: "Install".to_string(),
            reason: "Dependency".to_string(),
            object: HashMap::new(),
        };

        let preview = build_preview(&[local_user, dependency, upgrade, group]);

        assert_eq!(
            preview.additional_package_changes,
            vec![
                "Install libfoo-0:1.2.3-1.fc42.x86_64".to_string(),
                "Upgrade glibc-0:2.41-7.fc42.x86_64".to_string(),
            ]
        );
    }
}
