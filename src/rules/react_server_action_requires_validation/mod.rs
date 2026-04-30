mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-server-action-requires-validation",
    description: "Server Actions with parameters must validate input before use.",
    remediation: "Add `schema.parse(input)` or `schema.safeParse(input)` at the top of the Server Action body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
