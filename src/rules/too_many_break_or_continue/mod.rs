//! too-many-break-or-continue

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "too-many-break-or-continue",
    description: "Loop contains 2+ `break`/`continue` statements — consider refactoring.",
    remediation: "Extract the loop body into a function, use early returns, or restructure the logic. Multiple break/continue statements make loops hard to follow and often indicate the loop is doing too much.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
