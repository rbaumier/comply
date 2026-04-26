//! a11y-no-noninteractive-tabindex

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-noninteractive-tabindex",
    description: "Flag non-interactive elements with `tabIndex` (other than -1).",
    remediation: "Remove `tabIndex` from non-interactive elements or use a native interactive element. `tabIndex={-1}` is acceptable for programmatic focus.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
