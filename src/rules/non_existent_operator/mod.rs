//! non-existent-operator

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "non-existent-operator",
    description: "Typo operator detected — `=+`, `=-`, `=!` are not valid operators.",
    remediation: "Swap the characters: `=+` → `+=`, `=-` → `-=`, `=!` → `!=`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
