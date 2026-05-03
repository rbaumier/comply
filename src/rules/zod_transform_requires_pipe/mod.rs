//! zod-transform-requires-pipe — `.transform()` returns `z.any()` in
//! terms of the parser output. Without a following `.pipe(z.*)` the
//! schema silently produces an un-validated value. Requiring `.pipe()`
//! forces authors to re-assert the output type at the boundary.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-transform-requires-pipe",
    description: "`.transform()` returns an untyped value — follow with `.pipe(z.*)` to re-validate.",
    remediation: "Chain `.pipe(z.string())` (or the appropriate schema) after `.transform()` so the transformed value is validated.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
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
