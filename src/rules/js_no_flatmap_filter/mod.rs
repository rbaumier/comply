//! js-no-flatmap-filter — `.flatMap(...).filter(...)` iterates twice;
//! return an empty array from `flatMap` to combine map+filter in one pass.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-flatmap-filter",
    description: "`.flatMap().filter()` iterates twice — combine into a single `.flatMap()`.",
    remediation: "Move the filter predicate into the `flatMap` callback and return `[]` for \
                  items that should be excluded.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
