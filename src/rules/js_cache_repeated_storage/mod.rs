//! js-cache-repeated-storage — repeated `localStorage.getItem(key)` /
//! `sessionStorage.getItem(key)` with the same key in one function body.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "js-cache-repeated-storage",
    description: "Repeated `getItem()` calls with the same key — read once into a variable.",
    remediation: "Store the result of `localStorage.getItem(key)` in a variable and \
                  reuse it instead of calling `getItem` multiple times.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
