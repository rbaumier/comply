//! ts-no-useless-empty-export backend — detect `export {}` when other
//! `export` or `import` statements exist in the file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static EMPTY_EXPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*export\s*\{\s*\}\s*;?\s*$").unwrap()
});

static REAL_EXPORT_OR_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    // Match export/import statements that actually export or import something.
    // Excludes `export {}` (handled separately).
    Regex::new(r"(?:^|\s)(?:export\s+(?:default|const|let|var|function|class|type|interface|enum|async|abstract|\*|\{[^}]+\})|import\s)").unwrap()
});

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut empty_export_lines = Vec::new();
        let mut has_real_export = false;

        for (idx, line) in ctx.source.lines().enumerate() {
            if EMPTY_EXPORT.is_match(line) {
                empty_export_lines.push(idx + 1);
            } else if REAL_EXPORT_OR_IMPORT.is_match(line) {
                has_real_export = true;
            }
        }

        if !has_real_export {
            return Vec::new();
        }

        empty_export_lines
            .into_iter()
            .map(|line| Diagnostic {
                path: ctx.path.to_path_buf(),
                line,
                column: 1,
                rule_id: "ts-no-useless-empty-export".into(),
                message: "`export {}` is unnecessary — the file already has other exports."
                    .into(),
                severity: Severity::Warning,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_export_with_other_exports() {
        let diags = run("export const x = 1;\nexport {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_export_as_only_export() {
        assert!(run("const x = 1;\nexport {};").is_empty());
    }

    #[test]
    fn flags_empty_export_with_import() {
        let diags = run("import { foo } from 'bar';\nexport {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_no_empty_export() {
        assert!(run("export const x = 1;").is_empty());
    }
}
