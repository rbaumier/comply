//! ui-animate-presence-requires-exit — `<motion.*>` rendered under
//! `<AnimatePresence>` must declare an `exit` prop or it will snap out.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-animate-presence-requires-exit",
    description: "`<motion.*>` rendered inside `<AnimatePresence>` must define an `exit` prop to animate on unmount.",
    remediation: "Add `exit={{ ... }}` that mirrors the `initial` prop so the component animates out.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
