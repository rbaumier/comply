//! no-type-encoded-names — reject Hungarian notation.

mod rust;
mod type_prefix;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-type-encoded-names",
    description: "Identifiers must not encode their type (`strName`, `arrItems`).",
    remediation: "Remove the type prefix. TypeScript's type checker already \
                  tells you the type — encoding it in the name is obsolete \
                  and lies when the type changes.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
