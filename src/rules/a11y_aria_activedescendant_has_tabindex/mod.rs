//! a11y-aria-activedescendant-has-tabindex

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-activedescendant-has-tabindex",
    description: "Elements with `aria-activedescendant` must be tabbable.",
    remediation: "Add `tabIndex={0}` (or another non-negative value) to the element that uses `aria-activedescendant`.",
    severity: Severity::Error,
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
