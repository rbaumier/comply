//! elysia-scope-missing

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-scope-missing",
    description: "Plugin defines lifecycle hooks but no scope — hooks won't propagate to the parent app.",
    remediation: "Add `as: 'global'` or `as: 'scoped'` to the hook (or call `.as('scoped')` on the plugin) so hooks apply to the consumer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
