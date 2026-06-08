//! hono-cookie-no-samesite

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-samesite",
    description: "Cookie without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.",
    remediation: "Set `sameSite: 'Lax'` (default for most cases) or `sameSite: 'Strict'` for sensitive cookies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],

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
