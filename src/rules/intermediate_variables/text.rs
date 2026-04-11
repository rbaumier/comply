use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Count the number of operators on a line.
/// Operators counted: &&, ||, ??, +, -, *, /, ?, :
fn count_operators(line: &str) -> usize {
    let mut count = 0;
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip string literals
        if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'`' {
            let quote = bytes[i];
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1; // skip escaped char
                }
                i += 1;
            }
            i += 1; // skip closing quote
            continue;
        }

        // Two-char operators first
        if i + 1 < len {
            let two = &line[i..i + 2];
            if two == "&&" || two == "||" || two == "??" {
                count += 1;
                i += 2;
                continue;
            }
            // Skip arrow =>
            if two == "=>" {
                i += 2;
                continue;
            }
        }

        // Single-char operators (but not in contexts where they aren't operators)
        match bytes[i] {
            b'+' | b'-' | b'*' | b'/' => {
                // Skip ++ and -- (unary)
                if i + 1 < len
                    && bytes[i + 1] == bytes[i]
                    && (bytes[i] == b'+' || bytes[i] == b'-')
                {
                    i += 2;
                    continue;
                }
                // Skip // comments
                if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
                    return count; // rest of line is a comment
                }
                count += 1;
            }
            b'?' => {
                // standalone ? (ternary), already handled ?? above
                count += 1;
            }
            b':' => {
                count += 1;
            }
            _ => {}
        }
        i += 1;
    }
    count
}

/// Count chained method calls (`.foo(...)` sequences) on a single line.
/// E.g. `a.b.c.map(x => x * 2).filter(Boolean)` has 4 dot-accesses + 2 calls.
/// We count `.identifier(` patterns specifically.
fn count_chain_depth(line: &str) -> usize {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut depth = 0;
    let mut i = 0;

    while i < len {
        // Skip string literals
        if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'`' {
            let quote = bytes[i];
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }

        // Look for `.identifier(` pattern
        if bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_alphabetic() {
            // Walk past the identifier
            let start = i + 1;
            let mut j = start;
            while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            // Check if followed by `(`
            if j < len && bytes[j] == b'(' {
                depth += 1;
            }
            i = j;
            continue;
        }

        i += 1;
    }
    depth
}

/// Check if a line is a return statement or contains a function call with complex args.
fn is_return_or_call(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("return ") {
        return true;
    }
    // Check for function call pattern: identifier( ... with operators ...
    // We look for `name(` where the args contain the operators
    if trimmed.contains('(')
        && !trimmed.starts_with("function ")
        && !trimmed.starts_with("if ")
        && !trimmed.starts_with("while ")
        && !trimmed.starts_with("for ")
        && !trimmed.starts_with("switch ")
    {
        return true;
    }
    false
}

const OPERATOR_THRESHOLD: usize = 3;
const CHAIN_THRESHOLD: usize = 3;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let too_many_ops = is_return_or_call(line) && count_operators(line) >= OPERATOR_THRESHOLD;
            let too_deep_chain =
                is_return_or_call(line) && count_chain_depth(line) >= CHAIN_THRESHOLD;

            if too_many_ops || too_deep_chain {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "intermediate-variables".into(),
                    message:
                        "Expression is deeply nested — extract sub-expressions into named intermediate variables."
                            .into(),
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
    fn flags_complex_return() {
        let src = "function f() {\n  return a && b || c ?? d;\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn allows_simple_return() {
        let src = "function f() {\n  return a + b;\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_complex_function_call() {
        let src = "doSomething(a + b * c / d);\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_simple_call() {
        let src = "doSomething(a, b);\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_operators_in_strings() {
        let src = "return \"a && b || c ?? d + e\";\n";
        // Only the return is checked; operators inside the string should not count.
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deep_method_chain_in_return() {
        let src = "return a.b.map(x => x).filter(Boolean).join(',');\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_short_chain() {
        let src = "return items.filter(Boolean);\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deep_chain_in_call() {
        let src = "console.log(data.users.map(u => u.name).filter(Boolean).join(', '));\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }
}
