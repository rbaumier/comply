//! prefer-destructuring-assignment

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-destructuring-assignment",
    description: "Consecutive property accesses on the same object can be destructured.",
    remediation: "Use destructuring: `const { x, y } = obj;` instead of separate `const x = obj.x; const y = obj.y;` declarations. Destructuring is more concise and makes the intent clear.",
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
