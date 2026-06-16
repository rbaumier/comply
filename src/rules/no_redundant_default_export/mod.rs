//! no-redundant-default-export — a default export that re-exports the same
//! binding as a named export.
//!
//! When `export default foo` references the same binding that is already a
//! named export (`export const foo`, `export { foo }`, `export { foo as bar }`,
//! or `export { foo as default }`), the default export is redundant: consumers
//! can already import the symbol by name. Re-exports (`export … from "…"`) are
//! out of scope, and a default export of a fresh value or an anonymous
//! function/class is never redundant.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-default-export",
    description: "A default export references the same symbol as a named export.",
    remediation: "Remove either the default export or the named export so each \
                  symbol has a single canonical import path.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["complexity"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
