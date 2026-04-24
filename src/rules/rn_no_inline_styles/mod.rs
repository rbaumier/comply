//! rn-no-inline-styles — forbid inline `style={{ ... }}` objects.
//!
//! RN's `StyleSheet.create` interns styles and gives them stable references.
//! Inline objects re-allocate every render and defeat memoisation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-inline-styles",
    description: "Inline style objects allocate on every render and break memoisation.",
    remediation: "Move styles to `StyleSheet.create(...)` or wrap in `useMemo`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
