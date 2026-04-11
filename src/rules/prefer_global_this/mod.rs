//! prefer-global-this — prefer `globalThis` over `window`, `self`, and `global`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-global-this",
    description: "Prefer `globalThis` over `window`, `self`, and `global`.",
    remediation: "Replace `window.`, `self.`, or `global.` with `globalThis.`. \
                  `globalThis` is the standard cross-platform way to access the \
                  global object in any JS environment.",
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
