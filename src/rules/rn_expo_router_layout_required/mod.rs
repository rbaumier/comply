//! rn-expo-router-layout-required — each Expo Router group directory needs `_layout.tsx`.
//!
//! Expo Router relies on a `_layout` file at each routable directory level to
//! compose navigation. A file importing `expo-router` inside a directory
//! without `_layout.*` almost always indicates a routing bug.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-expo-router-layout-required",
    description: "Directories that import `expo-router` must contain a `_layout` file.",
    remediation: "Add `_layout.tsx` (or `.ts` / `.jsx` / `.js`) to the directory.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
