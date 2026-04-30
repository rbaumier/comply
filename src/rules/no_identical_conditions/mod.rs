//! no-identical-conditions

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-identical-conditions",
    description: "Duplicate condition in `if / else if` chain is always dead code or a bug.",
    remediation: "Change one of the duplicate conditions so each branch is reachable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
