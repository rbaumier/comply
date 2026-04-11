//! no-redundant-boolean

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-boolean",
    description: "Redundant boolean literal in a return or condition.",
    remediation: "Simplify: `if (x) return true; else return false;` \u{2192} `return x;`. `x === true` \u{2192} `x`. The boolean adds no information.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
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
