//! no-magic-array-flat-depth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-magic-array-flat-depth",
    description: "Disallow a magic number as the `depth` argument in `Array#flat()`.",
    remediation: "Extract the depth into a named constant, or use `Infinity` for unbounded flattening.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
