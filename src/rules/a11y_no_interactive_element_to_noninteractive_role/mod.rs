//! a11y-no-interactive-element-to-noninteractive-role

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-interactive-element-to-noninteractive-role",
    description: "Interactive elements must not be assigned non-interactive ARIA roles.",
    remediation: "Remove the non-interactive `role` or use a non-interactive element instead of `<button>`, `<a>`, `<input>`, `<select>`, or `<textarea>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
