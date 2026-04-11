use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if `++` or `--` is used inside a larger expression (not standalone).
/// Standalone patterns: `i++;`, `++i;`, `i--;`, `--i;`
/// Also allow in for-loop update clauses: `for (...; ...; i++)`
fn has_nested_incdec(line: &str) -> bool {
    let trimmed = line.trim();

    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return false;
    }

    // Check if line contains ++ or --
    if !trimmed.contains("++") && !trimmed.contains("--") {
        return false;
    }

    // Standalone statement: entire trimmed line is `ident++;` or `++ident;` or similar
    // Strip trailing semicolons and whitespace
    let stmt = trimmed.trim_end_matches(';').trim();
    if is_standalone_incdec(stmt) {
        return false;
    }

    // For-loop update clause: `for (...; ...; i++)` — skip these
    if is_for_loop_update(trimmed) {
        return false;
    }

    // If we get here, ++ or -- is embedded in a larger expression
    true
}

/// Check if the statement is just `i++`, `++i`, `i--`, or `--i`.
fn is_standalone_incdec(stmt: &str) -> bool {
    if stmt.starts_with("++") || stmt.starts_with("--") {
        let rest = stmt[2..].trim();
        return rest
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.');
    }
    if stmt.ends_with("++") || stmt.ends_with("--") {
        let rest = stmt[..stmt.len() - 2].trim();
        return rest
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.');
    }
    false
}

/// Check if the line is a for-loop and the ++ or -- is only in the update clause.
fn is_for_loop_update(line: &str) -> bool {
    let Some(for_pos) = line.find("for") else {
        return false;
    };
    let after_for = line[for_pos + 3..].trim_start();
    if !after_for.starts_with('(') {
        return false;
    }

    let paren_content = &after_for[1..];

    // Find the second semicolon (start of update clause) at depth 1
    let mut depth = 1i32;
    let mut semicolons = Vec::new();
    let mut close_paren = None;
    for (i, b) in paren_content.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close_paren = Some(i);
                    break;
                }
            }
            b';' if depth == 1 => semicolons.push(i),
            _ => {}
        }
    }

    if semicolons.len() < 2 || close_paren.is_none() {
        return false;
    }

    // Check that ++ / -- only appears in the update clause and the for body is on another line
    let before_update = &paren_content[..semicolons[1]];
    let after_close = &paren_content[close_paren.unwrap()..];

    // ++ or -- must NOT be in init/condition parts
    if before_update.contains("++") || before_update.contains("--") {
        return false;
    }

    // ++ or -- must NOT be in the body part after the for header (on this line)
    let body = after_close.trim_start_matches(')').trim();
    if body.starts_with('{') {
        let body_rest = &body[1..];
        if body_rest.contains("++") || body_rest.contains("--") {
            return false;
        }
    }

    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_nested_incdec(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-nested-incdec".into(),
                    message: "`++`/`--` inside an expression — separate into its own statement for clarity.".into(),
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
    fn flags_incdec_in_array_index() {
        assert_eq!(run("arr[i++] = x;").len(), 1);
    }

    #[test]
    fn flags_incdec_in_function_call() {
        assert_eq!(run("f(x++);").len(), 1);
    }

    #[test]
    fn flags_incdec_in_arithmetic() {
        assert_eq!(run("const y = a + b++;").len(), 1);
    }

    #[test]
    fn allows_standalone_postfix() {
        assert!(run("i++;").is_empty());
    }

    #[test]
    fn allows_standalone_prefix() {
        assert!(run("++i;").is_empty());
    }

    #[test]
    fn allows_for_loop_update() {
        assert!(run("for (let i = 0; i < n; i++) {").is_empty());
    }
}
