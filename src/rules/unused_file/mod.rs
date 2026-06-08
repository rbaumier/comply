//! unused-file — flag files unreachable from any project entry point.
//!
//! Cross-file rule that walks the import graph from a heuristic set of entry
//! points (framework dirs, root index/main, config files, package.json bin)
//! and emits a diagnostic on every indexed TS/JS/TSX file that BFS never
//! reached. Test files, declaration files, and config files are skipped — they
//! are loaded by tooling rather than imported by application code.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "unused-file",
    description: "File is not reachable from any entry point via the import graph.",
    remediation: "Delete the file if it's truly unused, or add an import from a reachable module.",
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
