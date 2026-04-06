use std::cmp::Ordering;

use rpm::rpm_evr_compare;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallRelation {
    NotInstalled,
    SameVersion,
    Upgrade,
    Downgrade,
}

pub struct RelationSummary<'a> {
    pub state_title: &'a str,
    pub state_subtitle: &'a str,
    pub fallback_context: &'a str,
}

pub struct ConfirmationCopy<'a> {
    pub heading: &'a str,
    pub body: &'a str,
    pub destructive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionMode {
    Install,
    Reinstall,
    Downgrade,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageIdentity {
    pub name: String,
    pub evr: String,
    pub arch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedState {
    pub relation: InstallRelation,
    pub installed_evr_arch: Option<String>,
}

pub fn classify_state(local: &PackageIdentity, installed: &[PackageIdentity]) -> ClassifiedState {
    let Some(installed_pkg) = pick_best_installed_match(local, installed) else {
        return ClassifiedState {
            relation: InstallRelation::NotInstalled,
            installed_evr_arch: None,
        };
    };

    let relation = match rpm_evr_compare(&local.evr, &installed_pkg.evr) {
        Ordering::Equal => InstallRelation::SameVersion,
        Ordering::Greater => InstallRelation::Upgrade,
        Ordering::Less => InstallRelation::Downgrade,
    };

    ClassifiedState {
        relation,
        installed_evr_arch: Some(format!("{}.{}", installed_pkg.evr, installed_pkg.arch)),
    }
}

fn pick_best_installed_match<'a>(
    local: &PackageIdentity,
    installed: &'a [PackageIdentity],
) -> Option<&'a PackageIdentity> {
    installed
        .iter()
        .filter(|pkg| pkg.name == local.name)
        .filter(|pkg| is_compatible_arch(&pkg.arch, &local.arch))
        .max_by(|a, b| compare_candidates(local, a, b))
}

fn is_compatible_arch(installed_arch: &str, local_arch: &str) -> bool {
    installed_arch == local_arch || installed_arch == "noarch" || local_arch == "noarch"
}

fn compare_candidates(
    local: &PackageIdentity,
    a: &PackageIdentity,
    b: &PackageIdentity,
) -> Ordering {
    let a_exact = a.arch == local.arch;
    let b_exact = b.arch == local.arch;

    match (a_exact, b_exact) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => rpm_evr_compare(&a.evr, &b.evr),
    }
}

pub fn action_for_relation(relation: &InstallRelation) -> ActionMode {
    match relation {
        InstallRelation::SameVersion => ActionMode::Reinstall,
        InstallRelation::Downgrade => ActionMode::Downgrade,
        InstallRelation::NotInstalled | InstallRelation::Upgrade => ActionMode::Install,
    }
}

impl InstallRelation {
    pub fn summary(&self) -> RelationSummary<'static> {
        match self {
            InstallRelation::NotInstalled => RelationSummary {
                state_title: "Ready to install",
                state_subtitle: "This package is not currently installed.",
                fallback_context: "Not installed",
            },
            InstallRelation::SameVersion => RelationSummary {
                state_title: "Reinstall available",
                state_subtitle: "The same version is already installed.",
                fallback_context: "Same version installed",
            },
            InstallRelation::Upgrade => RelationSummary {
                state_title: "Upgrade available",
                state_subtitle: "A previous version is installed and can be upgraded.",
                fallback_context: "Older version installed",
            },
            InstallRelation::Downgrade => RelationSummary {
                state_title: "Downgrade warning",
                state_subtitle: "A newer version is installed; this will downgrade it.",
                fallback_context: "Newer version installed",
            },
        }
    }

    pub fn confirmation_copy(&self) -> ConfirmationCopy<'static> {
        match self {
            InstallRelation::Downgrade => ConfirmationCopy {
                heading: "Confirm downgrade",
                body: "A newer version is already installed. Continue and downgrade to this local RPM?",
                destructive: true,
            },
            InstallRelation::SameVersion => ConfirmationCopy {
                heading: "Confirm reinstall",
                body: "This exact version is already installed. Continue with a reinstall?",
                destructive: false,
            },
            InstallRelation::Upgrade => ConfirmationCopy {
                heading: "Confirm install",
                body: "An older version is installed. Continue to upgrade using this local RPM?",
                destructive: false,
            },
            InstallRelation::NotInstalled => ConfirmationCopy {
                heading: "Confirm install",
                body: "Install this local RPM package?",
                destructive: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, evr: &str, arch: &str) -> PackageIdentity {
        PackageIdentity {
            name: name.to_string(),
            evr: evr.to_string(),
            arch: arch.to_string(),
        }
    }

    #[test]
    fn classifies_same_version() {
        let local = pkg("foo", "0:1.2.3-1", "x86_64");
        let state = classify_state(&local, &[pkg("foo", "0:1.2.3-1", "x86_64")]);
        assert_eq!(state.relation, InstallRelation::SameVersion);
        assert_eq!(action_for_relation(&state.relation), ActionMode::Reinstall);
    }

    #[test]
    fn classifies_upgrade_and_downgrade() {
        let local_new = pkg("foo", "0:1.2.4-1", "x86_64");
        let state_new = classify_state(&local_new, &[pkg("foo", "0:1.2.3-1", "x86_64")]);
        assert_eq!(state_new.relation, InstallRelation::Upgrade);

        let local_old = pkg("foo", "0:1.2.2-1", "x86_64");
        let state_old = classify_state(&local_old, &[pkg("foo", "0:1.2.3-1", "x86_64")]);
        assert_eq!(state_old.relation, InstallRelation::Downgrade);
        assert_eq!(
            action_for_relation(&state_old.relation),
            ActionMode::Downgrade
        );
    }

    #[test]
    fn ignores_unrelated_arch_or_name() {
        let local = pkg("foo", "0:1.2.3-1", "x86_64");
        let state = classify_state(
            &local,
            &[
                pkg("bar", "0:1.2.3-1", "x86_64"),
                pkg("foo", "0:1.2.3-1", "aarch64"),
            ],
        );
        assert_eq!(state.relation, InstallRelation::NotInstalled);
    }

    #[test]
    fn prefers_latest_evr_when_multiple_installed_candidates_exist() {
        let local = pkg("foo", "0:1.5.0-1", "x86_64");
        let state = classify_state(
            &local,
            &[
                pkg("foo", "0:1.2.0-1", "x86_64"),
                pkg("foo", "0:1.4.0-1", "x86_64"),
            ],
        );

        assert_eq!(state.relation, InstallRelation::Upgrade);
        assert_eq!(
            state.installed_evr_arch.as_deref(),
            Some("0:1.4.0-1.x86_64")
        );
    }

    #[test]
    fn prefers_exact_arch_over_noarch_even_if_noarch_is_newer() {
        let local = pkg("foo", "0:1.3.0-1", "x86_64");
        let state = classify_state(
            &local,
            &[
                pkg("foo", "0:1.2.0-1", "x86_64"),
                pkg("foo", "0:2.0.0-1", "noarch"),
            ],
        );

        assert_eq!(state.relation, InstallRelation::Upgrade);
        assert_eq!(
            state.installed_evr_arch.as_deref(),
            Some("0:1.2.0-1.x86_64")
        );
    }
}
