//! elysia-prefer-instance-plugin

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-prefer-instance-plugin",
    description: "Plugin defined as a `(app: Elysia) => app...` callback — Elysia instance plugins are preferred for type inference and deduplication.",
    remediation: "Define plugins as `new Elysia({ name: '...' })...` instances. Callback plugins lose deduplication and degrade type inference.",
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
