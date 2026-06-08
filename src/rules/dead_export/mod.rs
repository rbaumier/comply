//! dead-export — flag exported symbols with no importer in the project.
//!
//! A symbol that's exported but never imported from another file is dead
//! weight: it inflates the public surface of a module, ties maintainers to
//! an API no one uses, and hides from refactors that would otherwise delete
//! it. The index's per-symbol usage map is the authoritative oracle.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "dead-export",
    description: "Symbol is exported but never imported elsewhere in the project.",
    remediation: "Remove the export (and the symbol if unused internally), or verify the export is still needed for an external consumer. Unused exports bloat the module's public surface.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "imports"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
