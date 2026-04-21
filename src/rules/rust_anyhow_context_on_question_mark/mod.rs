//! rust-anyhow-context-on-question-mark — attach `.context(...)` to `?`.
//!
//! In application code, a bare `?` surfaces the library's raw error
//! to the user with no callsite information. Chaining `.context("…")`
//! before `?` keeps the error type intact but adds the "what we were
//! doing" breadcrumb that makes `anyhow::Error` / `eyre::Report`
//! worth using.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-anyhow-context-on-question-mark",
    description: "`?` without `.context()` produces bare error messages with no callsite information.",
    remediation: "Chain `.context(\"what you were doing\")` before `?` so errors carry actionable context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
