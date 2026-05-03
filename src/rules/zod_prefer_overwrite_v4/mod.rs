//! zod-prefer-overwrite-v4 — prefer `.overwrite()` over `.transform()` when the
//! transform returns a value of the same shape.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-overwrite-v4",
    description: "`.transform()` widens the output type to whatever the callback \
                  returns, which breaks `z.input` vs `z.output` parity. When the \
                  callback returns a value of the same shape, Zod v4's `.overwrite()` \
                  keeps the input type intact.",
    remediation: "Replace `.transform(fn)` with `.overwrite(fn)` whenever `fn` returns \
                  the same shape as its input (e.g. `s => s.trim()`, `n => Math.round(n)`).",
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
