mod react;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-pass-data-to-parent",
    description: "`useEffect` that only calls a parent callback to pass data up — lift state instead.",
    remediation: "Move the state to the parent component and pass down a setter, or restructure to avoid the effect.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
