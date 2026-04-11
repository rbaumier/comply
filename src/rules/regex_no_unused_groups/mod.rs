//! regex-no-unused-groups

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-unused-groups",
    description: "Named capturing group is defined but never referenced.",
    remediation: "Use the group via `.groups.name` or `$<name>` in a replacement, or convert to a non-capturing group `(?:...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
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
