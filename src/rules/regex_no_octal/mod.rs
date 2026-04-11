//! regex-no-octal

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-octal",
    description: "Octal escapes in regex (`\\1`, `\\12`) are ambiguous — they could be backreferences or octal character codes.",
    remediation: "Use named backreferences (`\\k<name>`) or explicit Unicode escapes (`\\u{...}`) instead of bare octal sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
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
