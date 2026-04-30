//! no-inferred-any

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-inferred-any",
    description: "Detect likely untyped patterns that infer `any`.",
    remediation: "Add an explicit type annotation or use `as T` / `satisfies T` after `JSON.parse()` and `.json()` calls. Avoid `const x: any =` — use a concrete type or `unknown`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
