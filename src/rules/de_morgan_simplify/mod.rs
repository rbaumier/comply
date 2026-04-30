//! de-morgan-simplify

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "de-morgan-simplify",
    description: "Apply De Morgan's law: `!(a && b)` is `!a || !b`, `!(a || b)` is `!a && !b`.",
    remediation: "Distribute the negation using De Morgan's law. `!(a && b)` becomes `!a || !b` and `!(a || b)` becomes `!a && !b`. The expanded form is easier to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
