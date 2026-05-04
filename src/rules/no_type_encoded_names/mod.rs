//! no-type-encoded-names — reject Hungarian notation.
//!
//! Flags identifiers that encode their type in the name: `strName`,
//! `arrItems`, `boolReady` in TS; `str_value`, `arr_items`, `bool_flag`
//! in Rust. The type system already knows the type — encoding it in
//! the identifier is obsolete and lies the moment the type changes.
//!
//! Detection is purely lexical (no type checker): the rule walks
//! identifier declarations and matches the name against a curated
//! list of unambiguous Hungarian prefixes. See `type_prefix.rs` for
//! the list and the rationale behind which prefixes are included
//! and which were rejected as faux amis (`fn`, `num`, `int`, `vec`,
//! `func` — all common descriptive uses).

mod oxc_typescript;
mod rust;
mod type_prefix;
#[cfg(test)]
mod typescript;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-type-encoded-names",
    description: "Identifiers must not encode their type (`strName`, `arrItems`).",
    remediation: "Remove the type prefix. TypeScript's type checker already \
                  tells you the type — encoding it in the name is obsolete \
                  and lies when the type changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
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
