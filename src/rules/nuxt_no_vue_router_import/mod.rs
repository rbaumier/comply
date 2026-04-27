//! nuxt-no-vue-router-import

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-vue-router-import",
    description: "Nuxt provides its own router via `useRouter`/`useRoute` auto-imports.",
    remediation: "Remove the `vue-router` import; rely on the auto-imported `useRouter()` / `useRoute()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nuxt"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
