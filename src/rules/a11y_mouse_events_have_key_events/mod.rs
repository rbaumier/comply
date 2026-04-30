//! a11y-mouse-events-have-key-events

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-mouse-events-have-key-events",
    description: "Flag `onMouseOver` without `onFocus` and `onMouseOut` without `onBlur`.",
    remediation: "Add `onFocus` alongside `onMouseOver` and `onBlur` alongside `onMouseOut` to ensure keyboard accessibility.",
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
