//! elysia-bearer-missing-www-auth

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-bearer-missing-www-auth",
    description: "Bearer auth 401/400 response without `WWW-Authenticate` header — RFC 6750 violation.",
    remediation: "Add `set.headers['WWW-Authenticate'] = 'Bearer realm=\"...\"'` before returning the 401/400.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
