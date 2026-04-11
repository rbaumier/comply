//! no-array-reduce

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-reduce",
    description: "`Array#reduce()` and `Array#reduceRight()` are not allowed.",
    remediation: "Use a `for` loop, `for...of`, or other array methods instead of `.reduce()` / `.reduceRight()` for better readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
