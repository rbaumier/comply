//! no-non-literal-fs-filename

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-non-literal-fs-filename",
    description: "Filesystem operations with non-literal filenames can lead to path traversal attacks.",
    remediation: "Use string literals for filenames, or validate / sanitize the path before passing it to `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
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
