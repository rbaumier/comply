use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const METHODS: &[&str] = &[
    ".replace(",
    ".replaceAll(",
    ".trim()",
    ".trimStart()",
    ".trimEnd()",
    ".toUpperCase()",
    ".toLowerCase()",
    ".substring(",
    ".slice(",
    ".concat(",
    ".padStart(",
    ".padEnd(",
    ".normalize(",
    ".repeat(",
];

/// Detect standalone string method calls whose return value is not used.
/// A call is "standalone" if the trimmed line starts with an identifier chain
/// followed by the method, and the line is a simple expression statement
/// (no `=`, `return`, `(` before it suggesting it's an argument, etc.).
fn has_useless_string_op(line: &str) -> bool {
    let trimmed = line.trim();

    // Must contain one of the target methods
    let method = match METHODS.iter().find(|&&m| trimmed.contains(m)) {
        Some(m) => *m,
        None => return false,
    };

    // The line must be an expression statement: identifier.method(...)
    // Not assigned, not returned, not used as argument.

    // Skip if line has assignment before the method
    if let Some(method_pos) = trimmed.find(method) {
        let before = &trimmed[..method_pos];
        // If there's an `=` (assignment) before the method, result is used
        // (but skip `==`, `!=`, `===`, `!==`)
        for (i, c) in before.char_indices() {
            if c == '=' {
                let after = before.get(i + 1..i + 2).unwrap_or("");
                let prev = if i > 0 {
                    before.get(i - 1..i).unwrap_or("")
                } else {
                    ""
                };
                if after != "=" && prev != "!" && prev != ">" && prev != "<" {
                    return false;
                }
            }
        }
    }

    // Skip if the line starts with `return`, `const`, `let`, `var`, `yield`
    if trimmed.starts_with("return ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("yield ")
    {
        return false;
    }

    // Skip if the line starts with common contexts where the result is used
    // e.g., inside function call: `foo(str.trim())` or array: `[str.trim()]`
    // Heuristic: if the first non-identifier character before the method is `(` or `[`, skip
    if let Some(method_pos) = trimmed.find(method) {
        let before = trimmed[..method_pos].trim();
        // The "before" should look like an identifier chain: `foo.bar` or `this.name`
        // If there's a `(` or `[` or `,` in the before part, it's likely an argument
        if before.contains('(') || before.contains('[') || before.contains(',') {
            return false;
        }
    }

    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_useless_string_op(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "useless-string-operation".into(),
                    message: "String method result is ignored \u{2014} strings are immutable, the return value must be used.".into(),
                    severity: Severity::Error,
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
    fn flags_standalone_trim() {
        assert_eq!(run("  name.trim();").len(), 1);
    }

    #[test]
    fn flags_standalone_replace() {
        assert_eq!(run(r#"  str.replace("a", "b");"#).len(), 1);
    }

    #[test]
    fn flags_standalone_to_upper() {
        assert_eq!(run("  title.toUpperCase();").len(), 1);
    }

    #[test]
    fn allows_assigned_trim() {
        assert!(run("  const cleaned = name.trim();").is_empty());
    }

    #[test]
    fn allows_returned_value() {
        assert!(run("  return name.trim();").is_empty());
    }

    #[test]
    fn allows_as_argument() {
        assert!(run("  console.log(name.trim());").is_empty());
    }
}
