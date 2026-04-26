//! react-no-initialize-state-in-effect

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-initialize-state-in-effect",
    description: "`useEffect` with empty deps that only calls a `setState` is redundant — initialize in `useState` directly.",
    remediation: "Use useState(initialValue) instead of setting state in effect with empty deps",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
