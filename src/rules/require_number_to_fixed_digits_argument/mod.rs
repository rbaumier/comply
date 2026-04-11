//! require-number-to-fixed-digits-argument

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "require-number-to-fixed-digits-argument",
    description: "Enforce using the digits argument with `Number#toFixed()`.",
    remediation: "Pass an explicit digits argument: `num.toFixed(0)`. The default is `0` but relying on it harms readability.",
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
