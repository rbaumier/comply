//! no-loop-func — ports typescript-eslint's `@typescript-eslint/no-loop-func`.
//! Flag function expressions declared inside loop bodies because their
//! captured variables are often the shared mutable loop binding rather
//! than per-iteration snapshots.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-loop-func",
    description: "Function declared inside a loop body — captured variables may be shared across iterations.",
    remediation: "Move the function outside the loop, or capture the loop variable with an IIFE / \
                  a `let`-bound per-iteration local so the closure sees a stable value.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-loop-func"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
