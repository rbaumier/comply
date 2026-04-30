//! a11y-control-has-associated-label

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-control-has-associated-label",
    description: "Interactive elements must have an accessible label.",
    remediation: "Add text content, `aria-label`, or `aria-labelledby` to `<button>`, `<input>`, `<select>`, and `<textarea>` elements. `<input type=\"hidden\">` is exempt.",
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
