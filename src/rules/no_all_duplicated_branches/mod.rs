//! no-all-duplicated-branches

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-all-duplicated-branches",
    description: "All branches have the same implementation — the conditional is pointless.",
    remediation: "Remove the conditional and keep just the body. Duplicated branches hide that the branching is no longer meaningful.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
