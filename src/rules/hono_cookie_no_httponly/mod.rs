//! hono-cookie-no-httponly

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-httponly",
    description: "Cookie set without `httpOnly` — accessible to JavaScript (XSS vector).",
    remediation: "Add `httpOnly: true` to cookie options: `setCookie(c, name, value, { httpOnly: true, secure: true, sameSite: 'Lax' })`.",
    severity: Severity::Error,
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
