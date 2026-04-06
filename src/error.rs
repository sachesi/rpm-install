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

    #[error("Package installation was canceled")]
    InstallationCanceled,

    #[error("Reinstall is not supported by the active PackageKit backend")]
    ReinstallNotSupported,

    #[error("Downgrade is not supported by the active PackageKit backend")]
    DowngradeNotSupported,

    #[error("PackageKit reported an error ({code}): {details}")]
    PackageKit { code: u32, details: String },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;
