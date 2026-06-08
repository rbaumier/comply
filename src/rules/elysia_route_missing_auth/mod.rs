//! elysia-route-missing-auth

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-missing-auth",
    description: "Sensitive routes (e.g. `/admin`, `/me`, `/profile`) lack an auth guard.",
    remediation: "Add a `beforeHandle` auth check or wrap the route in `.guard({ auth: ... })` before serving sensitive paths.",
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
