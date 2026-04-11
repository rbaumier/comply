//! no-unenclosed-multiline-block

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-unenclosed-multiline-block",
    description: "`if`/`for`/`while` without braces and a multiline body is a bug magnet.",
    remediation: "Always wrap `if`/`for`/`while` bodies in curly braces `{}` when the body is on the next line.",
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
