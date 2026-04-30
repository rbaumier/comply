//! no-mutable-exports

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutable-exports",
    description: "Mutable export binding (`let`/`var`) — use `const` instead.",
    remediation: "Change `export let` or `export var` to `export const`. Mutable exports are confusing to consumers and hard to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
