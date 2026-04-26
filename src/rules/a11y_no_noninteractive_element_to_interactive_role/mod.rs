//! a11y-no-noninteractive-element-to-interactive-role

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-noninteractive-element-to-interactive-role",
    description: "Non-interactive elements must not be assigned interactive ARIA roles.",
    remediation: "Use a native interactive element (`<button>`, `<a>`) instead of adding an interactive `role` to a `<div>`, `<span>`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
