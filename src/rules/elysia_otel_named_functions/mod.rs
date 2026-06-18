//! elysia-otel-named-functions

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-otel-named-functions",
    description: "Anonymous arrow functions in `.derive` / `.resolve` produce unnamed OpenTelemetry spans, gutting trace readability.",
    remediation: "Pass a named function (e.g. `function deriveUser({ ... }) { ... }`) so OTEL emits a meaningful span name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["observability", "elysia"],

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
