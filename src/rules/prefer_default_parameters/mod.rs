//! prefer-default-parameters — prefer default parameters over reassignment.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-default-parameters",
    description: "Prefer default parameters over reassignment.",
    remediation: "Replace `x = x || 'default'` / `x = x ?? 'default'` in the \
                  function body with a default parameter value `function f(x = 'default')`. \
                  Default parameters are clearer and avoid subtle bugs with falsy values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
