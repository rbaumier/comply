//! rn-flashlist-estimated-item-size — `<FlashList>` requires `estimatedItemSize`.
//!
//! Without `estimatedItemSize`, FlashList falls back to measuring and logs a
//! runtime warning. Providing it is required for production performance.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rn-flashlist-estimated-item-size",
    description: "`<FlashList>` is missing the `estimatedItemSize` prop.",
    remediation: "Add `estimatedItemSize={<px>}` (approximate row height).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react-native"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
