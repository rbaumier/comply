//! ts-no-non-null-asserted-optional-chain — flag `(x?.y)!`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-optional-chain",
    description: "Non-null assertion after optional chain contradicts its purpose.",
    remediation: "Remove the `!` — the optional chain already handles the nullish case.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
