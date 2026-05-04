//! jsdoc-missing-example — exported functions with JSDoc must include `@example`.
//!
//! `jsdoc-on-exported` ensures the doc block exists; this rule ensures it
//! actually shows the caller HOW to use the function. The coding-standards
//! skill: "JSDoc on every exported function — block description + @example
//! with call AND return". A description without an example forces every
//! reader to imagine the call site.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-missing-example",
    description: "Exported function JSDoc must include an @example block.",
    remediation: "Add an `@example` block under the description showing a real \
                  call AND its return value: `@example\\n  const r = foo(42);\\n  // => 'forty-two'`. \
                  Examples are the fastest way for callers to understand the API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
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
