//! ts-unified-signatures — require function overload signatures to be merged.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-unified-signatures",
    description: "Function overload signatures that differ by a single parameter should be unified with a union or optional parameter.",
    remediation: "Merge the overload signatures into one using a union type or an optional parameter.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/unified-signatures"),
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
