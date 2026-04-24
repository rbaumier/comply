//! rn-flashlist-over-flatlist — prefer `FlashList` over `FlatList`.
//!
//! `@shopify/flash-list` produces dramatically better scroll performance than
//! the built-in `react-native` FlatList on most devices.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-flashlist-over-flatlist",
    description: "Importing `FlatList` from `react-native` is discouraged; use FlashList.",
    remediation: "Import `FlashList` from `@shopify/flash-list`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
