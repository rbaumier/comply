//! a11y-aria-props

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-props",
    description: "Flag invalid `aria-*` attributes in JSX.",
    remediation: "Use only valid WAI-ARIA attributes. Remove or replace the invalid `aria-` attribute with a valid one from the WAI-ARIA specification.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
