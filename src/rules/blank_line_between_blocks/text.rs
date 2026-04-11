use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_return_statement(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("return ") || t == "return;" || t.starts_with("return;")
}

fn is_declaration(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("const ") || t.starts_with("let ")
}

fn is_function_call(line: &str) -> bool {
    let t = line.trim();
    // Simple heuristic: line contains `(` and is not a declaration, return, if, for, while, etc.
    if t.is_empty() || t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
        return false;
    }
    if t.starts_with("const ") || t.starts_with("let ") || t.starts_with("var ")
        || t.starts_with("return") || t.starts_with("if ")
        || t.starts_with("for ") || t.starts_with("while ")
        || t.starts_with("switch ") || t.starts_with("}")
        || t.starts_with("else") || t.starts_with("{")
    {
        return false;
    }
    t.contains('(')
}

fn is_closing_brace(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('}')
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if idx == 0 {
                continue;
            }

            let prev = lines[idx - 1];

            // Rule 1: `return` preceded by a non-return, non-blank, non-`}` line.
            if is_return_statement(line)
                && !is_blank(prev)
                && !is_closing_brace(prev)
                && !is_return_statement(prev)
                && !prev.trim().starts_with("//")
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "blank-line-between-blocks".into(),
                    message: "Add a blank line before `return` for visual separation.".into(),
                    severity: Severity::Warning,
                });
            }

            // Rule 2: declaration immediately followed by a function call without blank line.
            if is_declaration(line) && idx + 1 < lines.len() {
                // Check the line AFTER the current declaration line.
            }
            if is_function_call(line) && is_declaration(prev) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "blank-line-between-blocks".into(),
                    message: "Add a blank line between declarations and function calls.".into(),
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
    fn flags_return_without_blank_line() {
        let src = "  const x = 1;\n  return x;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_return_after_blank_line() {
        let src = "  const x = 1;\n\n  return x;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_return_after_closing_brace() {
        let src = "  }\n  return x;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_declaration_then_call() {
        let src = "  const x = getX();\n  doSomething(x);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_declaration_then_call_with_blank() {
        let src = "  const x = getX();\n\n  doSomething(x);";
        assert!(run(src).is_empty());
    }
}
