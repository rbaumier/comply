//! no-redundant-optional backend — `?:` already implies `| undefined`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("?:") && trimmed.contains("| undefined") {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-redundant-optional".into(),
                message:
                    "`?:` already implies `| undefined` — remove the redundant union member."
                        .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_optional_with_undefined() {
        assert_eq!(run_on("  name?: string | undefined;").len(), 1);
    }

    #[test]
    fn flags_optional_with_undefined_complex() {
        assert_eq!(run_on("  value?: number | null | undefined;").len(), 1);
    }

    #[test]
    fn allows_optional_without_undefined() {
        assert!(run_on("  name?: string;").is_empty());
    }

    #[test]
    fn allows_required_with_undefined() {
        assert!(run_on("  name: string | undefined;").is_empty());
    }
}
