//! react-jsx-no-target-blank — missing rel="noreferrer" with target="_blank".

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-target-blank",
    description: "`target=\"_blank\"` without `rel=\"noreferrer\"` is a security risk.",
    remediation: "Add `rel=\"noreferrer\"` (or `rel=\"noopener noreferrer\"`) when \
                  using `target=\"_blank\"`. Without it, the opened page can access \
                  `window.opener` and redirect the parent page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
