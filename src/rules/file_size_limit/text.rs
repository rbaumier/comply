use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DEFAULT_LIMIT: usize = 3000;
const HELPER_LIMIT: usize = 500;
const HELPER_PATTERNS: &[&str] = &["utils", "helpers", "util", "helper"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let line_count = ctx.source.lines().count();
        let path_str = ctx.path.to_string_lossy();
        let path_lower = path_str.to_lowercase();

        let limit = if HELPER_PATTERNS.iter().any(|p| path_lower.contains(p)) {
            HELPER_LIMIT
        } else {
            DEFAULT_LIMIT
        };

        if line_count <= limit {
            return Vec::new();
        }

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "file-size-limit".into(),
            message: format!(
                "File has {line_count} lines (limit: {limit}) — consider splitting into smaller modules."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_with_path(path: &str, lines: usize) -> Vec<Diagnostic> {
        let source: String = (0..lines).map(|i| format!("line {i}\n")).collect();
        Check.check(&CheckCtx::for_test(Path::new(path), &source))
    }

    #[test]
    fn allows_small_file() {
        assert!(run_with_path("src/main.rs", 200).is_empty());
    }

    #[test]
    fn flags_large_file() {
        let diags = run_with_path("src/engine.rs", 3001);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3001"));
    }

    #[test]
    fn allows_file_at_limit() {
        assert!(run_with_path("src/main.rs", 3000).is_empty());
    }

    #[test]
    fn flags_helper_file_over_500() {
        let diags = run_with_path("src/utils.ts", 501);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("500"));
    }

    #[test]
    fn allows_helper_file_under_500() {
        assert!(run_with_path("src/helpers.ts", 499).is_empty());
    }
}
