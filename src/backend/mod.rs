pub mod dnf5daemon;
pub mod types;
pub mod zypper;

use std::path::Path;
use types::{BackendOperation, TransactionPreview};
use crate::error::AppResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendType {
    Dnf5,
    Zypper,
}

pub fn detect_backend() -> BackendType {
    if Path::new("/usr/bin/zypper").exists() {
        BackendType::Zypper
    } else {
        BackendType::Dnf5
    }
}

pub async fn preview_local_rpm_transaction(
    spec: &str,
    operation: BackendOperation,
) -> AppResult<TransactionPreview> {
    match detect_backend() {
        BackendType::Dnf5 => dnf5daemon::preview_local_rpm_transaction(spec, operation).await,
        BackendType::Zypper => zypper::preview_local_rpm_transaction(spec, operation).await,
    }
}

pub async fn run_local_rpm_transaction<F>(
    spec: &str,
    operation: BackendOperation,
    on_progress: F,
) -> AppResult<BackendOperation>
where
    F: FnMut(Option<u32>) + 'static,
{
    match detect_backend() {
        BackendType::Dnf5 => dnf5daemon::run_local_rpm_transaction(spec, operation, on_progress).await,
        BackendType::Zypper => zypper::run_local_rpm_transaction(spec, operation, on_progress).await,
    }
}
