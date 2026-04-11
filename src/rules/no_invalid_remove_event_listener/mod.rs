//! no-invalid-remove-event-listener

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-remove-event-listener",
    description: "`removeEventListener` with an inline function or `.bind()` call never matches the original listener.",
    remediation: "Pass a stable function reference to `removeEventListener` — store the bound/arrow function in a variable first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
