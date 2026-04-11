//! max-function-lines — cap every function at 30 lines.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "max-function-lines",
    description: "Functions longer than 30 lines mix abstraction levels.",
    remediation: "Function exceeds 30 lines. Extract a named helper for the \
                  tail of the body — one level of abstraction per function.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::too_many_lines")
}
