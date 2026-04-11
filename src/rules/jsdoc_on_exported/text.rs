//! jsdoc-on-exported — Vue text backend.
//!
//! Scans Vue SFC `<script>` sections for exported functions missing JSDoc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::is_vue_file;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // Look for exported functions/consts without preceding JSDoc.
            let is_export = trimmed.starts_with("export function ")
                || trimmed.starts_with("export const ")
                || trimmed.starts_with("export async function ")
                || trimmed.starts_with("export default function ");

            if !is_export {
                continue;
            }

            // Check if the preceding non-blank line ends a JSDoc block.
            let has_jsdoc = (0..i).rev().any(|j| {
                let prev = lines[j].trim();
                if prev.is_empty() {
                    return false;
                }
                // JSDoc closing line
                prev.ends_with("*/")
            });

            if !has_jsdoc {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: "jsdoc-on-exported".into(),
                    message: "Exported function is missing a JSDoc block.".into(),
                    severity: Severity::Warning,
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_export_without_jsdoc() {
        let src = "<script setup>\nexport function doStuff() {}\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_export_with_jsdoc() {
        let src = "<script setup>\n/** Does stuff. */\nexport function doStuff() {}\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("file.ts"),
            "export function doStuff() {}",
        ));
        assert!(diags.is_empty());
    }
}
