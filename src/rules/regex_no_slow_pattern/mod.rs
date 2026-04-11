//! regex-no-slow-pattern

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-slow-pattern",
    description: "Regex has nested quantifiers that can cause catastrophic backtracking (ReDoS).",
    remediation: "Refactor to avoid nested quantifiers like `(a+)+`, `(a*)*`, `(a+)*`, `(.*)*`. Use atomic groups, possessive quantifiers, or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "regex"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY_AND_RUST
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
