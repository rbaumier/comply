use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if the line declares a function whose name starts with
/// `find` or `get` (case-sensitive, camelCase convention).
fn is_find_or_get_function(line: &str) -> bool {
    let trimmed = line.trim();
    // Match: function findX / function getX / findX( / getX(
    for keyword in &["function find", "function get"] {
        if trimmed.contains(keyword) {
            return true;
        }
    }
    // Arrow / method shorthand: `findUser(`, `getUser(`
    for prefix in &["find", "get"] {
        if let Some(pos) = trimmed.find(prefix) {
            let after_prefix = &trimmed[pos + prefix.len()..];
            // Next char must be uppercase (camelCase) and eventually have `(`
            if after_prefix
                .chars()
                .next()
                .map_or(false, |c| c.is_uppercase())
                && after_prefix.contains('(')
            {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Simple heuristic: find function declarations named find*/get*,
        // then scan ahead for `return null` or `return undefined` in the body.
        let mut i = 0;
        while i < lines.len() {
            if is_find_or_get_function(lines[i]) {
                let fn_line = i;
                let mut brace_depth: i32 = 0;
                let mut entered_body = false;
                let mut has_null_return = false;

                // Scan the function body.
                for j in i..lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '{' {
                            brace_depth += 1;
                            entered_body = true;
                        } else if ch == '}' {
                            brace_depth -= 1;
                        }
                    }
                    let trimmed = lines[j].trim();
                    if trimmed.contains("return null") || trimmed.contains("return undefined") {
                        has_null_return = true;
                    }
                    if entered_body && brace_depth <= 0 {
                        i = j + 1;
                        break;
                    }
                    if j == lines.len() - 1 {
                        i = j + 1;
                    }
                }

                if has_null_return {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: fn_line + 1,
                        column: 1,
                        rule_id: "option-vs-result".into(),
                        message: "Function named `find*`/`get*` returns `null`/`undefined` — consider using an Option type to make absence explicit.".into(),
                        severity: Severity::Warning,
                    });
                }
            } else {
                i += 1;
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
    fn flags_find_returning_null() {
        let src = r#"
function findUser(id: string) {
    if (!id) return null;
    return db.get(id);
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_returning_undefined() {
        let src = r#"
function getConfig(key: string) {
    if (!map.has(key)) return undefined;
    return map.get(key);
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_without_null_return() {
        let src = r#"
function findUser(id: string) {
    return db.get(id);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_find_get_functions() {
        let src = r#"
function createUser(name: string) {
    if (!name) return null;
    return { name };
}
"#;
        assert!(run(src).is_empty());
    }
}
