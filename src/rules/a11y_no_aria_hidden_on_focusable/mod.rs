//! a11y-no-aria-hidden-on-focusable

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-aria-hidden-on-focusable",
    description: "Flag `aria-hidden=\"true\"` on focusable elements.",
    remediation: "Remove `aria-hidden` from focusable elements or remove the focusable behavior. Elements hidden from assistive technology should not be focusable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
