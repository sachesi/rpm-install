use thiserror::Error;

use crate::backend::types::BackendOperation;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("The provided path is not a local file: {0}")]
    NonLocalPath(String),

    #[error("Only .rpm files are supported")]
    UnsupportedFileType,

    #[error("Directories are not supported")]
    DirectoryNotSupported,

    #[error("Package appears to be a source RPM and is not installable through this GUI")]
    SourceRpmNotInstallable,

    #[error("dnf5daemon is unavailable: {0}")]
    DaemonUnavailable(String),

    #[error("Authentication was denied")]
    AuthDenied,

    #[error("Authentication was canceled")]
    AuthCanceled,

    #[error("Transaction was canceled")]
    TransactionCanceled,

    #[error("Operation {0:?} is not supported by this dnf5daemon runtime")]
    UnsupportedOperation(BackendOperation),

    #[error("The selected RPM could not be read or is invalid: {0}")]
    InvalidLocalRpm(String),

    #[error("{operation:?} failed: {details}")]
    OperationFailed {
        operation: BackendOperation,
        details: String,
    },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;
