//! top-level-function — prefer `function foo() {}` over
//! `const foo = () => {}` at module top-level.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "top-level-function",
    description: "Top-level arrow-function variables hide their name in stack traces and \
                  prevent hoisting — use a function declaration instead.",
    remediation: "Replace `const foo = (…) => { … }` at module scope with \
                  `function foo(…) { … }`. Keep arrow functions for callbacks and inline expressions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["style"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
