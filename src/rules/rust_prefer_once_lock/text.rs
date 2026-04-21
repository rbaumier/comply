//! rust-prefer-once-lock backend.
//!
//! Line-scans for `lazy_static!` macro invocations and `once_cell::sync::{Lazy,OnceCell}`
//! references. `LazyLock`/`OnceLock` from `std::sync` are the supported
//! replacements since Rust 1.70 and carry none of the third-party weight.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            let flagged = t.starts_with("lazy_static!")
                || t.contains("once_cell::sync::Lazy")
                || t.contains("once_cell::sync::OnceCell");
            if flagged {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `std::sync::LazyLock` or `OnceLock` (stable since Rust 1.70) instead of `lazy_static!` or `once_cell`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_lazy_static_macro() {
        assert_eq!(run("lazy_static! { static ref FOO: String = String::new(); }").len(), 1);
    }

    #[test]
    fn flags_once_cell_lazy() {
        assert_eq!(run("static FOO: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| compute());").len(), 1);
    }

    #[test]
    fn allows_std_once_lock() {
        assert!(run("static FOO: std::sync::OnceLock<String> = std::sync::OnceLock::new();").is_empty());
    }

    #[test]
    fn allows_lazy_lock() {
        assert!(run("static FOO: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| compute());").is_empty());
    }
}
