//! no-return-type-any backend — functions with explicit `: any` return type.

use crate::diagnostic::{Diagnostic, Severity};

/// Detect ): any { or ): any => or ): Promise<any>
fn has_return_type_any(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed.contains("): any {") || trimmed.contains("): any =>") || trimmed.contains("): any;")
    {
        return true;
    }

    if trimmed.contains("): Promise<any>") {
        return true;
    }

    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
        if has_return_type_any(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-return-type-any".into(),
                message: "Function has explicit `: any` return type — use a specific type or `unknown`.".into(),
                severity: Severity::Error,
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
    fn flags_any_return_function() {
        assert_eq!(run_on("function foo(): any {").len(), 1);
    }

    #[test]
    fn flags_any_return_arrow() {
        assert_eq!(run_on("const foo = (): any => {};").len(), 1);
    }

    #[test]
    fn flags_promise_any_return() {
        assert_eq!(run_on("async function foo(): Promise<any> {").len(), 1);
    }

    #[test]
    fn allows_specific_return_type() {
        assert!(run_on("function foo(): string {").is_empty());
    }

    #[test]
    fn allows_unknown_return() {
        assert!(run_on("function foo(): unknown {").is_empty());
    }
}
