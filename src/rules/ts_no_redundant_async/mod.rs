//! ts-no-redundant-async — an `async` function whose only useful work is
//! `return await expr;` and which has no try/catch can drop the `async` and
//! `await` and just return the inner promise.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-redundant-async",
    description: "`async function f() { return await x; }` is redundant — the wrapper adds no behaviour over `function f() { return x; }`.",
    remediation: "Drop `async` and `await`, or keep them only when you need a try/catch \
                  around the awaited expression.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "async"],
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
