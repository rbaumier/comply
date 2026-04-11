//! ts-no-dynamic-delete — disallow `delete` on computed key expressions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-dynamic-delete",
    description: "Using `delete` on a computed key is error-prone — use `Map` or `Set` instead.",
    remediation: "Remove the dynamic `delete` and use a `Map`/`Set`, or delete a static key.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-dynamic-delete/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
