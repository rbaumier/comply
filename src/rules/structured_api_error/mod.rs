//! structured-api-error

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "structured-api-error",
    description: "Bare `new Error()` in route handlers — use structured errors.",
    remediation: "Replace `new Error(\"message\")` with a structured error containing `{ type, code, status, detail }`. Bare Error messages are not machine-parseable and lack HTTP status context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
