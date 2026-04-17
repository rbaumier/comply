//! require-explicit-undefined

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "require-explicit-undefined",
    description: "Functions that return a value must use `return undefined;` — bare `return;` hides intent.",
    remediation: "Replace bare `return;` with `return undefined;` inside functions whose return type is not `void` or `never`. The explicit form makes the undefined value a deliberate choice, not an accident.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
