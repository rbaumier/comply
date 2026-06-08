//! elysia-prefer-redirect

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-prefer-redirect",
    description: "Manual redirect via `set.status = 301/302` and `set.headers.location` — use `redirect()` for typed redirects.",
    remediation: "Return `redirect(url, code)` from the handler instead of mutating `set.status` and `set.headers.location`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],

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
