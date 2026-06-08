//! elysia-cookie-getter-setter

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-getter-setter",
    description: "Code uses `cookie.get(...)` / `cookie.set(...)` — Elysia exposes cookies as `cookie.<name>.value`.",
    remediation: "Use `ctx.cookie.<name>.value` for reads and assignment for writes (e.g. `cookie.session.value = '...'`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],

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
