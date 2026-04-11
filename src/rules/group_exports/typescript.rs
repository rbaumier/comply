//! group-exports backend — flag multiple named export declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

fn is_named_export(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("export ") {
        return false;
    }
    let rest = &trimmed[7..];
    if rest.starts_with("default ") || rest.starts_with('{') || rest.starts_with('*') {
        return false;
    }
    true
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut export_lines: Vec<usize> = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_named_export(line) {
                export_lines.push(idx + 1);
            }
        }
        if export_lines.len() <= 1 {
            return Vec::new();
        }
        export_lines[1..]
            .iter()
            .map(|&line_num| Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line_num,
                column: 1,
                rule_id: "group-exports".into(),
                message: "Multiple named export declarations — consolidate into \
                          a single export block."
                    .into(),
                severity: Severity::Warning,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_multiple_exports() {
        let src = "export const a = 1;\nexport const b = 2;\nexport function foo() {}\n";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_single_export() {
        let src = "export const a = 1;\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_export_default() {
        let src = "export const a = 1;\nexport default function main() {}\n";
        assert!(run_on(src).is_empty());
    }
}
