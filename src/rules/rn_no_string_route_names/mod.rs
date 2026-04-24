//! rn-no-string-route-names — ban string route names in `navigation.navigate(...)`.
//!
//! Expo Router provides typed paths via `router.push('/path')`; passing a bare
//! string route name bypasses that type-checking.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-no-string-route-names",
    description: "`navigation.navigate('Name', ...)` bypasses Expo Router's typed paths.",
    remediation: "Use `router.push('/typed/path')` from expo-router instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
