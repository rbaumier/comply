//! rust-thiserror-for-lib backend.
//!
//! Skips `main.rs` and `src/bin/` (application crates tend to use
//! `anyhow::Error` instead of a typed enum) and skips any file that
//! already mentions `thiserror`. Any remaining `pub enum *Error`
//! declaration is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path_str = ctx.path.to_string_lossy();
        if path_str.contains("main.rs") || path_str.contains("src/bin/") { return vec![]; }
        if ctx.source.contains("thiserror") { return vec![]; }

        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("pub enum ") && t.contains("Error") && !t.starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `#[derive(thiserror::Error)]` for library error types — avoids boilerplate `Display` impls.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("src/error.rs"), src))
    }

    #[test]
    fn flags_pub_enum_error_without_thiserror() {
        assert_eq!(run("pub enum MyError { NotFound, Unauthorized }").len(), 1);
    }

    #[test]
    fn allows_enum_with_thiserror() {
        assert!(run("#[derive(thiserror::Error)]\npub enum MyError { #[error(\"not found\")] NotFound }").is_empty());
    }

    #[test]
    fn ignores_main_rs() {
        let ctx = crate::rules::backend::CheckCtx::for_test(
            Path::new("src/main.rs"),
            "pub enum MyError { Fail }",
        );
        assert!(Check.check(&ctx).is_empty());
    }
}
