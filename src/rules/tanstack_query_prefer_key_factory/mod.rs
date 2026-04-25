

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-prefer-key-factory",
    description: "Inline dynamic `queryKey` arrays should use a key factory for consistency.",
    remediation: "Define a key factory: `const todoKeys = { detail: (id: string) => ['todos', id] as const }` and use `todoKeys.detail(id)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
