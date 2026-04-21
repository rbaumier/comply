mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-inline-default-prop",
    description: "Non-primitive default props in `memo()` create new references every render, breaking memoization.",
    remediation: "Define the default value outside the component: `const EMPTY: T[] = []` then `{ items = EMPTY }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
