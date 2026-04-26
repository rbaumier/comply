//! Heuristic: in a member `Cargo.toml` (a manifest that is NOT itself
//! the workspace root — i.e. no `[workspace]` section), any entry in
//! `[dependencies]`, `[dev-dependencies]` or `[build-dependencies]`
//! must reference `workspace = true`. A plain `foo = "1.2.3"` or
//! table entry without `workspace = true` is flagged.
//!
//! The check scans line-by-line — it doesn't round-trip TOML — because
//! we only need to recognise section headers and simple `key = ...`
//! lines. Anything fancier (array-of-tables, nested inline tables) is
//! either already fine or gets a benign pass.

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

        // A root workspace manifest defines `[workspace]`. We don't
        // enforce the rule on the root itself — only on member crates.
        let is_workspace_root = ctx
            .source
            .lines()
            .any(|l| l.trim_start().starts_with("[workspace]"));
        if is_workspace_root {
            return diagnostics;
        }

        let mut in_deps_section = false;
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();

            // New section header resets state.
            if trimmed.starts_with('[') {
                let section = trimmed.trim_end_matches('\r');
                in_deps_section = matches!(
                    section,
                    "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
                );
                continue;
            }

            if !in_deps_section {
                continue;
            }

            // Skip comments and blank lines.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // We only care about `key = ...` lines.
            let Some(eq_idx) = trimmed.find('=') else {
                continue;
            };
            let value = trimmed[eq_idx + 1..].trim();

            if value.contains("workspace = true") || value.contains("workspace=true") {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "dependency pinned in a member crate — use `{ workspace = true }` \
                          and declare the version in `[workspace.dependencies]`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_pinned_version_in_member_crate() {
        let src = "[package]\nname = \"foo\"\n\n[dependencies]\nserde = \"1.0\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_workspace_inherited_dep() {
        let src = "[package]\nname = \"foo\"\n\n[dependencies]\nserde = { workspace = true }\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_workspace_root() {
        let src = "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nserde = \"1.0\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_dev_dependency_pinning() {
        let src = "[package]\nname = \"foo\"\n\n[dev-dependencies]\ntempfile = \"3\"\n";
        assert_eq!(run(src).len(), 1);
    }
}
