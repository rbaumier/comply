//! no-inferred-any

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-inferred-any",
    description: "Detect likely untyped patterns that infer `any`.",
    remediation: "Add an explicit type annotation or use `as T` / `satisfies T` after `JSON.parse()` and `.json()` calls. Avoid `const x: any =` — use a concrete type or `unknown`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
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
