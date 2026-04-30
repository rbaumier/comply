//! no-hook-setter-in-body

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-hook-setter-in-body",
    description: "`useState` setter called directly in component body causes infinite re-renders.",
    remediation: "Move the setter call inside `useEffect`, `useCallback`, or an event handler.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
