//! no-misplaced-loop-counter

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-misplaced-loop-counter",
    description: "`for` loop update clause modifies a different variable than the condition.",
    remediation: "Ensure the update expression (`i++`) modifies the same variable used in the loop condition (`i < n`). Mismatched variables usually indicate a copy-paste bug.",
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
