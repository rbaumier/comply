//! a11y-dialog-missing-aria-labelledby — flag `role="dialog"` /
//! `<dialog>` / Dialog component openings without any of `aria-label`
//! or `aria-labelledby`. A dialog without a name is unannounced.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-dialog-missing-aria-labelledby",
    description: "Dialog elements without `aria-label` / `aria-labelledby` are unannounced.",
    remediation: "Add `aria-label` or point `aria-labelledby` at the dialog title element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
