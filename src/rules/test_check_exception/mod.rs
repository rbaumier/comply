//! test-check-exception

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "test-check-exception",
    description: "`.toThrow()` without specifying what to check.",
    remediation: "Specify the expected error: `.toThrow(TypeError)`, `.toThrow('message')`, or `.toThrow(/regex/)`. Bare `.toThrow()` passes for any error, hiding bugs.",
    severity: Severity::Warning,
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
