use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Scan for `function isX(`: find the signature line returning `: boolean`,
/// then look ahead in the body for `typeof` or `instanceof`.
impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let len = lines.len();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Match `function isX(` returning `: boolean`
            if !trimmed.contains("function is") {
                continue;
            }
            if !trimmed.contains(": boolean") {
                continue;
            }
            // Verify it's a function declaration like `function isX(`
            if let Some(fn_pos) = trimmed.find("function is") {
                let after_fn = &trimmed[fn_pos + "function is".len()..];
                // The next char should be uppercase (isX pattern) or at least alphanumeric
                if let Some(first_char) = after_fn.chars().next() {
                    if !first_char.is_ascii_alphanumeric() {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Look ahead in the body for typeof or instanceof (up to 30 lines)
            let search_end = (idx + 30).min(len);
            let mut has_type_check = false;
            for body_line in &lines[idx..search_end] {
                if body_line.contains("typeof ") || body_line.contains("instanceof ") {
                    has_type_check = true;
                    break;
                }
            }

            if has_type_check {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-type-guard".into(),
                    message: "Function `isX` returns `boolean` with type checks — use a type predicate (`x is Type`) instead.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_is_function_with_typeof() {
        let src = r#"
function isString(x: unknown): boolean {
    return typeof x === "string";
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_is_function_with_instanceof() {
        let src = r#"
function isError(x: unknown): boolean {
    return x instanceof Error;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_type_predicate() {
        let src = r#"
function isString(x: unknown): x is string {
    return typeof x === "string";
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_is_function() {
        let src = r#"
function checkValue(x: unknown): boolean {
    return typeof x === "string";
}
"#;
        assert!(run(src).is_empty());
    }
}
