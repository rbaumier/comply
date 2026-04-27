//! nuxt-no-manual-imports

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-manual-imports",
    description: "Nuxt auto-imports composables — manual imports are redundant.",
    remediation: "Remove the import; Nuxt auto-imports `useRuntimeConfig`, `useState`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nuxt"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
