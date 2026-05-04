//! elysia-eden-error-unchecked

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-eden-error-unchecked",
    description: "Eden treaty calls return `{ data, error }` — destructuring only `data` swallows the error path.",
    remediation: "Destructure both `{ data, error }` and check `error` before consuming `data`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
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
