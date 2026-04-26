mod react;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-use-state-initializer-function",
    description: "Expensive `useState` initial values should use a lazy initializer `() => expr`.",
    remediation: "Replace `useState(expensiveCall())` with `useState(() => expensiveCall())` so the computation only runs once.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
