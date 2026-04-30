//! prefer-less-than — suggest rewriting `b > a` as `a < b` for readability.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-less-than",
    description: "Prefer `<` / `<=` over `>` / `>=` for readability.",
    remediation: "Prefer `<` over `>` for readability",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
