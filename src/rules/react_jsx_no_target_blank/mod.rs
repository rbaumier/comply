//! react-jsx-no-target-blank — missing rel="noreferrer" with target="_blank".

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
