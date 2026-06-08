//! hono-no-unvalidated-body — flag `c.req.json()` / `c.req.parseBody()` without validation.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-no-unvalidated-body",
    description: "Reading the request body without a validator middleware skips schema validation and can let malformed input reach handlers.",
    remediation: "Use `validator('json', schema)` (or `zValidator`, `tbValidator`, etc.) and read the parsed body via `c.req.valid('json')`.",
    severity: Severity::Warning,
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
