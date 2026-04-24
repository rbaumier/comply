//! ts-no-large-string-union — flag string literal unions exceeding 50 members.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-large-string-union",
    description: "String-literal union has more than 50 members; consider a branded string or enum.",
    remediation: "Replace the union with a branded string type, a const object + `keyof typeof`, or an enum. Huge unions slow the compiler and produce useless error messages.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
