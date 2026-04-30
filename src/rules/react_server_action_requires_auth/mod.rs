mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-server-action-requires-auth",
    description: "Server Actions with mutations must check authentication.",
    remediation: "Call `getSession()` or `auth()` and verify the result before performing mutations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
