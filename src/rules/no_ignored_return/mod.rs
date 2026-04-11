//! no-ignored-return

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-ignored-return",
    description: "Return value of a pure method is ignored — the call has no effect.",
    remediation: "Assign or return the result: `const result = arr.map(...)` or use a side-effect method instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
