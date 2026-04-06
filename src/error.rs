use thiserror::Error;

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

    #[error("Authentication was denied or canceled")]
    AuthCanceled,

    #[error("Transaction was canceled")]
    TransactionCanceled,

    #[error("The requested operation is not supported by dnf5daemon: {0}")]
    UnsupportedOperation(String),

    #[error("Installation failed: {0}")]
    InstallFailure(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;
