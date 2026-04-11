//! comment-prose-quality

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "comment-prose-quality",
    description: "Comments with weasel words, passive voice, or lexical illusions \
                  reduce clarity.",
    remediation: "Rewrite the comment to be direct. Replace passive voice with \
                  active. Remove filler words. Fix repeated words.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
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
