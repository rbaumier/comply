//! Backend enum — how a rule is enforced for a given language.
//!
//! A rule can be enforced several different ways per language:
//! - **TreeSitter**: in-process AST walk via a tree-sitter grammar. Maximum
//!   control, zero-subprocess latency, works for any language comply bundles
//!   a grammar for.
//! - **Text**: plain-text / regex / filesystem check. No AST needed — used
//!   for line counts, TODO scans, filename conventions. // comply-ignore: todo-needs-issue-link — mention, not marker.
//! - **Oxlint**: delegation to an oxlint rule. Comply registers the oxlint
//!   rule-id in the runtime-generated oxlintrc, then remaps the resulting
//!   diagnostic's rule-id + message back to our RuleMeta. From the user's
//!   perspective the rule belongs to comply.
//! - **Clippy** (v2): delegation to a clippy lint, same remap pattern.
//! - **Tsc** (v1.2): shell out to `tsc --noEmit`, filter diagnostics by error
//!   code. Used for type-aware rules.
//!
//! The engine picks the backend for `(rule, language)` and invokes it once
//! per file. TreeSitter and Text backends produce diagnostics directly;
//! Oxlint/Clippy/Tsc backends don't produce anything at check-time — instead
//! they contribute their rule-id to the generated config for the external
//! tool, and their diagnostics are remapped post-hoc.

use crate::config::Config;
#[cfg(test)]
use crate::config::default_static_config;
use crate::diagnostic::Diagnostic;
use std::path::Path;

/// Read-only context handed to in-process check implementations.
///
/// `config` is the resolved per-project configuration. Rules that
/// expose thresholds (max-function-lines, law-of-demeter depth, …)
/// read their knobs from here via `config.threshold(rule_id, key, fallback)`,
/// so a project's `comply.toml` can override the defaults without
/// touching any rule code.
#[derive(Debug)]
pub struct CheckCtx<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub config: &'a Config,
}

impl<'a> CheckCtx<'a> {
    /// Convenience constructor for unit tests. Uses the process-wide
    /// default config (defaults only, no user `comply.toml`), so test
    /// files don't need to construct one and pass it through manually.
    /// Production code should always go through engine.rs which builds
    /// a real `CheckCtx` with the loaded config.
    #[cfg(test)]
    pub fn for_test(path: &'a Path, source: &'a str) -> Self {
        Self {
            path,
            source,
            config: default_static_config(),
        }
    }
}

/// A tree-sitter-backed check — receives a parsed AST.
pub trait AstCheck: Send + Sync {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic>;
}

/// A text-only check — no AST needed.
pub trait TextCheck: Send + Sync {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic>;
}

/// How a rule is enforced for one language.
///
/// `Debug` is hand-written below rather than derived: the `TreeSitter`
/// and `Text` variants carry `Box<dyn AstCheck>` / `Box<dyn TextCheck>`
/// trait objects, and adding `Debug` to those traits would force every
/// concrete check struct to implement Debug AND thread the bound through
/// the trait surface. The manual impl labels the variant and elides the
/// inner check, which is enough for diagnostics + assert messages.
#[non_exhaustive]
#[allow(dead_code)] // Oxlint/Clippy/Tsc variants land in later steps.
pub enum Backend {
    /// In-process tree-sitter AST walk.
    TreeSitter(Box<dyn AstCheck>),
    /// Plain-text / regex / filesystem check.
    Text(Box<dyn TextCheck>),
    /// Delegate to an oxlint rule. Comply enables the rule in the generated
    /// oxlintrc and remaps oxlint's diagnostic back to our RuleMeta.
    Oxlint { rule: &'static str },
    /// (v2) Delegate to a clippy lint — same remap pattern as Oxlint.
    Clippy { lint: &'static str },
    /// (v1.2) Shell out to `tsc --noEmit` and filter by diagnostic code.
    Tsc { codes: &'static [u32] },
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TreeSitter(_) => f.write_str("Backend::TreeSitter(<dyn AstCheck>)"),
            Self::Text(_) => f.write_str("Backend::Text(<dyn TextCheck>)"),
            Self::Oxlint { rule } => write!(f, "Backend::Oxlint {{ rule: {rule:?} }}"),
            Self::Clippy { lint } => write!(f, "Backend::Clippy {{ lint: {lint:?} }}"),
            Self::Tsc { codes } => write!(f, "Backend::Tsc {{ codes: {codes:?} }}"),
        }
    }
}
