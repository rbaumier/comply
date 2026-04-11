//! cyclomatic-complexity

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "cyclomatic-complexity",
    description: "Functions with cyclomatic complexity > 10 are hard to test and maintain.",
    remediation: "Refactor the function: extract helper functions, use early returns, replace conditionals with polymorphism or lookup tables.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
