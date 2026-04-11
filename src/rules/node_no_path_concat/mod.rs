//! node-no-path-concat

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "node-no-path-concat",
    description: "String concatenation with `__dirname` / `__filename` is platform-dependent.",
    remediation: "Use `path.join()` or `path.resolve()` instead of string concatenation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
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
