//! no-mutable-exports

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-mutable-exports",
    description: "Mutable export binding (`let`/`var`) — use `const` instead.",
    remediation: "Change `export let` or `export var` to `export const`. Mutable exports are confusing to consumers and hard to reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
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
