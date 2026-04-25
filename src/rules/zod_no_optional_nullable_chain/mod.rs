//! zod-no-optional-nullable-chain — collapse `.optional().nullable()` to `.nullish()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-optional-nullable-chain",
    description: "`.optional().nullable()` should be written as `.nullish()` for clarity.",
    remediation: "Replace `.optional().nullable()` or `.nullable().optional()` with `.nullish()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
