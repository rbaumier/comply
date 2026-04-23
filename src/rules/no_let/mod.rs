//! no-let

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-let",
    description: "Disallow `let` declarations — prefer `const` for immutable bindings.",
    remediation: "Replace `let` with `const`. If you truly need to reassign, restructure the code to use a new binding, `reduce`, or a pure function instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
