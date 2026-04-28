//! react-no-memo-default-value — `memo(({ items = [] }) => ...)` defeats `memo`
//! because the default `[]` / `{}` is a fresh reference on every render.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-memo-default-value",
    description: "`memo(Component)` with default `[]` / `{}` parameters re-renders every time.",
    remediation: "Extract default values to a module-level constant so the \
                  reference is stable across renders.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
