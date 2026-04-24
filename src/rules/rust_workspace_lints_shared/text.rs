//! Two complementary checks keyed off whether the manifest looks like
//! a workspace root or a member crate:
//!
//! - **Root manifest** (contains `[workspace]`): must also contain a
//!   `[workspace.lints` section header (e.g. `[workspace.lints.clippy]`).
//! - **Member manifest** (has `[package]` but no `[workspace]`): must
//!   contain a `[lints]` section with `workspace = true`.
//!
//! Both diagnostics anchor on line 1 because the absence of a section
//! has no meaningful line number.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if ctx.path.file_name().is_none_or(|n| n != "Cargo.toml") {
            return diagnostics;
        }

        let has_workspace = ctx
            .source
            .lines()
            .any(|l| l.trim_start().starts_with("[workspace]"));
        let has_package = ctx
            .source
            .lines()
            .any(|l| l.trim_start().starts_with("[package]"));
        let has_workspace_lints = ctx
            .source
            .lines()
            .any(|l| l.trim_start().starts_with("[workspace.lints"));

        if has_workspace && !has_workspace_lints {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "workspace root is missing `[workspace.lints.*]` — \
                          declare a shared lint policy (clippy/rust/rustdoc) \
                          so member crates can inherit it."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        // Member crate check: has [package] but no [workspace]. It must
        // declare `[lints]` followed by `workspace = true`.
        if has_package && !has_workspace {
            let mut in_lints = false;
            let mut inherits = false;
            for line in ctx.source.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('[') {
                    in_lints = trimmed.starts_with("[lints]");
                    continue;
                }
                if in_lints && trimmed.replace(' ', "").starts_with("workspace=true") {
                    inherits = true;
                    break;
                }
            }
            if !inherits {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "member crate does not inherit workspace lints — \
                              add `[lints]` and `workspace = true` to share the \
                              workspace's lint policy."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Cargo.toml"), source))
    }

    #[test]
    fn flags_workspace_without_shared_lints() {
        let src = "[workspace]\nmembers = [\"a\"]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_member_crate_missing_lints_inherit() {
        let src = "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_workspace_with_shared_lints() {
        let src = "[workspace]\nmembers = [\"a\"]\n\n[workspace.lints.clippy]\nall = \"warn\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_member_that_inherits_lints() {
        let src = "[package]\nname = \"foo\"\n\n[lints]\nworkspace = true\n";
        assert!(run(src).is_empty());
    }
}
