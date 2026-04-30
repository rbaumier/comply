//! a11y-no-noninteractive-element-to-interactive-role

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    RuleDef {
        meta: META,
        backends,
    }
}
