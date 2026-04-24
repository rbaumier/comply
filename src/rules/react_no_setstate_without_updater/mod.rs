//! react-no-setstate-without-updater — `setX(x + 1)` pattern missing the functional updater.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-setstate-without-updater",
    description: "State setter called with an expression that reads the current state \
                  variable directly — races against concurrent updates in React 18+.",
    remediation: "Use the functional updater: `setX(prev => prev + 1)` or \
                  `setX(prev => [...prev, item])`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
