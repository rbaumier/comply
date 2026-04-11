//! no-anonymous-default-export — disallow anonymous default exports.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-anonymous-default-export",
    description: "Disallow anonymous functions and classes as the default export.",
    remediation: "Name the exported function or class. Anonymous default \
                  exports break refactoring tools, produce unhelpful stack \
                  traces, and make `import` auto-complete less useful.",
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
