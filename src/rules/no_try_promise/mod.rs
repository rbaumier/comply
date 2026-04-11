//! no-try-promise

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-try-promise",
    description: "Promise rejection inside try/catch without `await` won't be caught.",
    remediation: "Add `await` before promise-returning calls inside try blocks, or use `.catch()` directly. Without `await`, the promise rejects asynchronously and the `catch` block never runs.",
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
