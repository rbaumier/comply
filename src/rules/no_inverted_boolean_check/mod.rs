//! no-inverted-boolean-check

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-inverted-boolean-check",
    description: "`!a === b` negates `a` before comparing — likely meant `a !== b`.",
    remediation: "The `!` operator binds tighter than `===`/`!==`, so `!a === b` is `(!a) === b`, not `!(a === b)`. Use `a !== b` or wrap explicitly: `!(a === b)`.",
    severity: Severity::Warning,
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
