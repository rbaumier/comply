//! no-incomplete-assertions

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-incomplete-assertions",
    description: "Assertion chain is missing the actual matcher.",
    remediation: "Complete the assertion with a matcher: `expect(x).toBe(...)`, `.toEqual(...)`, `.toThrow()`, etc. Bare `expect(x);` or `expect(x).not;` tests nothing.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
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
