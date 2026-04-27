//! nuxt-no-client-only-in-ssr

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-client-only-in-ssr",
    description: "Browser globals (`window`, `document`, `localStorage`) crash on the server without a client guard.",
    remediation: "Wrap the access in `if (import.meta.client)` / `if (process.client)`, move it to `onMounted`, or use `<ClientOnly>`.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/api/components/client-only"),
    categories: &["nuxt", "ssr"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
