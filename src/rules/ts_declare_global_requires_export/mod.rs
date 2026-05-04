//! ts-declare-global-requires-export — files with `declare global` need
//! a top-level `export {}` to be treated as modules.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-declare-global-requires-export",
    description: "`declare global` only augments the global scope when the file is a module; needs at least `export {}`.",
    remediation: "Add `export {};` at the end of the file so TypeScript treats it as a module and the `declare global` block takes effect.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
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
