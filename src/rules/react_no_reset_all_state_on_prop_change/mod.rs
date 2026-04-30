mod react;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-reset-all-state-on-prop-change",
    description: "`useEffect` resets multiple states when a prop changes — use a key instead.",
    remediation: "Add a `key={prop}` to the component to reset all state automatically when the prop changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
