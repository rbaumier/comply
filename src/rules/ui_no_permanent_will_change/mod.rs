//! ui-no-permanent-will-change — flag inline `willChange` styles other than
//! `'auto'`. `will-change` should be applied dynamically right before an
//! animation and removed after; leaving it permanently wastes GPU memory.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-permanent-will-change",
    description: "Inline `willChange` is permanent — `will-change` should be applied dynamically, \
                  not baked into static styles.",
    remediation: "Apply `will-change` only during the active animation (e.g. on hover/focus) and \
                  remove it after, or set it to `'auto'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
