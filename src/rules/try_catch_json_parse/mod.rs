//! try-catch-json-parse — flag `JSON.parse(...)` outside a try block.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "try-catch-json-parse",
    description: "`JSON.parse` can throw — wrap it in try/catch or a Result helper.",
    remediation: "Wrap `JSON.parse(input)` in a try/catch, or use a safe parser \
                  (Zod, `Result.try`, etc). Any invalid or empty input throws a \
                  SyntaxError that will crash the request/event handler.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],
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
