//! elysia-global-with-types

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-global-with-types",
    description: "Plugin uses `as: 'global'` while also exposing `state`, `decorate`, or `model` — global scope leaks types into every consumer.",
    remediation: "Use `as: 'scoped'` for plugins that publish typed context. Reserve `'global'` for hook-only plugins (logging, telemetry).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],

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
