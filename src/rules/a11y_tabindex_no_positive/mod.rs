//! a11y-tabindex-no-positive

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-tabindex-no-positive",
    description: "`tabIndex` must not be positive — only `0` or `-1` are valid.",
    remediation: "Use `tabIndex={0}` to make an element focusable in document order, or `tabIndex={-1}` for programmatic focus only. Positive values break natural tab order.",
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
