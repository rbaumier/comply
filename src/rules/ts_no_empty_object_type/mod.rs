//! ts-no-empty-object-type — flag `{}` used as a type.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-empty-object-type",
    description: "`{}` as a type matches any non-nullish value — it almost never means what you think.",
    remediation: "Use `Record<string, never>` for an empty object, `object` for any object, \
                  or `unknown` for any value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
