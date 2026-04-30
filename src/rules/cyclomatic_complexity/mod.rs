//! cyclomatic-complexity

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "cyclomatic-complexity",
    description: "Functions with cyclomatic complexity > 10 are hard to test and maintain.",
    remediation: "Refactor the function: extract helper functions, use early returns, replace conditionals with polymorphism or lookup tables.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
