#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendOperation {
    Install,
    Upgrade,
    Reinstall,
    Downgrade,
}
