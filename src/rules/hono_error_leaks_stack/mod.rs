//! hono-error-leaks-stack — flag error handlers that return `err.stack` / `err.message`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-error-leaks-stack",
    description: "Returning `err.stack` or `err.message` from `app.onError(...)` leaks internal details to clients.",
    remediation: "Return a generic message (e.g. `c.json({ error: 'Internal Server Error' }, 500)`) and log the original error server-side.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["hono", "security"],

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
