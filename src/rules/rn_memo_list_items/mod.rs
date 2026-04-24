//! rn-memo-list-items — list item components should be wrapped in `React.memo`.
//!
//! Heuristic: when a `renderItem` prop references an identifier, we verify
//! the same file wraps that component in `React.memo(...)` or `memo(...)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-memo-list-items",
    description: "List item components referenced by `renderItem` should be wrapped in React.memo.",
    remediation: "Wrap the component definition in `memo(...)` / `React.memo(...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
