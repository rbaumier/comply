//! ts-no-confusing-non-null-assertion — flag `a! == b` (looks like `a !== b`).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-confusing-non-null-assertion",
    description: "`a! == b` looks confusingly like `a !== b`.",
    remediation: "Remove the `!` or wrap the left side in parentheses: `(a!) == b`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
