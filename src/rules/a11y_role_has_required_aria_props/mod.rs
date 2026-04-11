//! a11y-role-has-required-aria-props

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-role-has-required-aria-props",
    description: "Elements with ARIA roles must have all required ARIA properties.",
    remediation: "Add the missing ARIA properties: `checkbox`/`radio` need `aria-checked`, `slider` needs `aria-valuenow`/`aria-valuemin`/`aria-valuemax`, `combobox` needs `aria-expanded`, `scrollbar` needs `aria-controls`/`aria-valuenow`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
