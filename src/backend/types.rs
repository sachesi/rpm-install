use crate::state_logic::{ActionMode, InstallRelation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendOperation {
    Install,
    Reinstall,
    Upgrade,
    Downgrade,
}

impl BackendOperation {
    pub fn label(self) -> &'static str {
        match self {
            BackendOperation::Install | BackendOperation::Upgrade | BackendOperation::Downgrade => {
                "Install"
            }
            BackendOperation::Reinstall => "Reinstall",
        }
    }

    pub fn verb_past(self) -> &'static str {
        match self {
            BackendOperation::Install => "Installed",
            BackendOperation::Reinstall => "Reinstalled",
            BackendOperation::Upgrade => "Upgraded",
            BackendOperation::Downgrade => "Downgraded",
        }
    }
}

pub fn operation_for_relation(relation: &InstallRelation) -> BackendOperation {
    match relation {
        InstallRelation::NotInstalled => BackendOperation::Install,
        InstallRelation::SameVersion => BackendOperation::Reinstall,
        InstallRelation::Upgrade => BackendOperation::Upgrade,
        InstallRelation::Downgrade => BackendOperation::Downgrade,
    }
}

pub fn action_mode_for_operation(operation: BackendOperation) -> ActionMode {
    match operation {
        BackendOperation::Reinstall => ActionMode::Reinstall,
        BackendOperation::Downgrade => ActionMode::Downgrade,
        BackendOperation::Install | BackendOperation::Upgrade => ActionMode::Install,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_expected_operation_for_relation() {
        assert_eq!(
            operation_for_relation(&InstallRelation::NotInstalled),
            BackendOperation::Install
        );
        assert_eq!(
            operation_for_relation(&InstallRelation::SameVersion),
            BackendOperation::Reinstall
        );
        assert_eq!(
            operation_for_relation(&InstallRelation::Upgrade),
            BackendOperation::Upgrade
        );
        assert_eq!(
            operation_for_relation(&InstallRelation::Downgrade),
            BackendOperation::Downgrade
        );
    }
}
