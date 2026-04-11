//! no-hidden-control-flow

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-hidden-control-flow",
    description: "3+ decorators stacked on a single function/class hide control flow.",
    remediation: "Reduce the decorator stack to 2 or fewer. Each decorator adds invisible control flow — stacking 3+ makes the execution path hard to reason about. Compose decorators into a single higher-level one or use explicit middleware.",
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
