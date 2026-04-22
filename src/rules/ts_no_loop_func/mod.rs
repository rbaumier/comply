//! ts-no-loop-func — disallow function declarations/expressions inside loops.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-loop-func",
    description: "Functions declared inside loops often cause bugs due to closures capturing the loop variable by reference.",
    remediation: "Move the function outside the loop, or use `let`/`const` in a `for` loop to create a new binding per iteration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-loop-func"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
