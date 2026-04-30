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

mod rust;
mod type_prefix;
mod typescript;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
