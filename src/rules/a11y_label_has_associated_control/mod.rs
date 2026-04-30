//! a11y-label-has-associated-control

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-label-has-associated-control",
    description: "`<label>` must have an associated control via `htmlFor` or by wrapping an input.",
    remediation: "Add `htmlFor=\"input-id\"` to the `<label>` or wrap an `<input>`, `<select>`, or `<textarea>` inside it.",
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
