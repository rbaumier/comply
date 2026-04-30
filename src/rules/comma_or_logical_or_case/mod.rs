//! comma-or-logical-or-case

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comma-or-logical-or-case",
    description: "Switch `case` uses comma or `||` instead of fall-through.",
    remediation: "Use separate `case` clauses with fall-through instead of comma or `||` in a single `case`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
