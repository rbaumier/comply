//! nestjs-pipe-must-implement-transform — pipes must implement `transform()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-pipe-must-implement-transform",
    description: "Classes implementing `PipeTransform` must define a `transform()` method.",
    remediation: "Implement the `transform(value, metadata)` method required by `PipeTransform`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
