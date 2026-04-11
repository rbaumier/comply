//! pure-by-default

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "pure-by-default",
    description: "Function references top-level mutable state.",
    remediation: "Pass the state as a parameter instead of referencing a top-level `let`/`var`. This makes the function pure and easier to test.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
