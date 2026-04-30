//! ts-assertion-fn-must-be-declaration — assertion functions cannot be arrows.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-assertion-fn-must-be-declaration",
    description: "Assertion functions (`asserts x`) cannot be arrow functions — TypeScript requires a function declaration or method.",
    remediation: "Rewrite the arrow as a `function` declaration: `function assertX(...): asserts x is T { ... }`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
