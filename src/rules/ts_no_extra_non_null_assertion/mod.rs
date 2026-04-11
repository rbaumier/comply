//! ts-no-extra-non-null-assertion — flag `x!!` (double non-null assertion).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-extra-non-null-assertion",
    description: "Extra non-null assertions (`!!`) are redundant and confusing.",
    remediation: "Remove the extra `!` — a single non-null assertion is sufficient.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
