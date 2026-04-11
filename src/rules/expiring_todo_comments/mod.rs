//! expiring-todo-comments — TODO/FIXME with expired date conditions.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "expiring-todo-comments",
    description: "TODO/FIXME with an expiration date that has passed should be resolved.",
    remediation: "Resolve the TODO/FIXME — the expiration date has passed. \
                  Either complete the task or update the date.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .copied()
            .chain(std::iter::once(Language::Rust))
            .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
