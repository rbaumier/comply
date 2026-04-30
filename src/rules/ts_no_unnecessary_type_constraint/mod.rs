//! ts-no-unnecessary-type-constraint — flag `<T extends any>` or `<T extends unknown>`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unnecessary-type-constraint",
    description: "`<T extends any>` and `<T extends unknown>` are unnecessary — all types already extend these.",
    remediation: "Remove the `extends any` or `extends unknown` constraint.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
