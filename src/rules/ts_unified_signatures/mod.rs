//! ts-unified-signatures — require function overload signatures to be merged.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
    crate::register_ts_family!(META, typescript)
}
