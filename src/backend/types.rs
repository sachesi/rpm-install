use crate::state_logic::InstallRelation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendOperation {
    Install,
    Reinstall,
    Upgrade,
    Downgrade,
    Remove,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionPreview {
    pub additional_package_changes: Vec<String>,
}

impl BackendOperation {
    pub fn label(self) -> &'static str {
        match self {
            BackendOperation::Install | BackendOperation::Upgrade | BackendOperation::Downgrade => {
                "Install"
            }
            BackendOperation::Reinstall => "Reinstall",
            BackendOperation::Remove => "Uninstall",
        }
    }

    pub fn verb_past(self) -> &'static str {
        match self {
            BackendOperation::Install => "Installed",
            BackendOperation::Reinstall => "Reinstalled",
            BackendOperation::Upgrade => "Upgraded",
            BackendOperation::Downgrade => "Downgraded",
            BackendOperation::Remove => "Removed",
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
