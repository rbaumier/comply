//! duplicate-export — flag symbols re-exported by multiple barrel files.
//!
//! When the same name is re-exported from two or more barrels, importers can
//! reach it through several paths. That ambiguity scatters the public surface,
//! makes refactors harder, and lets cyclic graphs hide. The import index sees
//! every re-export, so one anchored pass per project is enough.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "duplicate-export",
    description: "Same symbol is re-exported by multiple barrel files, creating ambiguous import paths.",
    remediation: "Remove the duplicate re-export from one of the barrels so each symbol has a single canonical import path.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports", "code-quality"],
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
