mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-client-framework-import",
    description: "Import `createAuthClient` from a framework-specific path (e.g. `better-auth/react`).",
    remediation: "Replace `better-auth/client` with `better-auth/react`, `better-auth/vue`, `better-auth/svelte`, or `better-auth/solid`.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/installation"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
