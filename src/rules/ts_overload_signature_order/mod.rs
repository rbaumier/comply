//! ts-overload-signature-order — overloads ordered specific-to-general.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-overload-signature-order",
    description: "Function overload signatures should be ordered from most specific to most general.",
    remediation: "Reorder the overloads so earlier signatures have more parameters or narrower types; TypeScript matches top-to-bottom and a general signature first shadows the specific one.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
