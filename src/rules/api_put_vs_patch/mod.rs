//! api-put-vs-patch — flag handlers registered via `app.put(...)` / router
//! PUT methods whose body references `Partial<...>`. PUT must replace the
//! full resource; partial updates belong on PATCH. Mixing the two breaks
//! idempotency guarantees clients rely on.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-put-vs-patch",
    description: "PUT handlers with Partial<> payloads should use PATCH instead.",
    remediation: "If the handler accepts fields-provided-only semantics, register it with `.patch(...)`. Keep `.put(...)` for full-resource replacement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
