use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const THRESHOLD: usize = 10;

/// Count complexity-incrementing tokens in a line.
fn line_complexity(line: &str) -> usize {
    let trimmed = line.trim();

    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return 0;
    }

    let mut count = 0;

    // Keyword-based branches — match as whole words
    for keyword in &[
        "if ", "if(", "else if ", "else if(", "while ", "while(", "for ", "for(", "catch ",
        "catch(", "case ",
    ] {
        let mut start = 0;
        while let Some(pos) = trimmed[start..].find(keyword) {
            let abs = start + pos;
            // Ensure it's at a word boundary (start of line or preceded by non-alphanumeric)
            if abs == 0 || !trimmed.as_bytes()[abs - 1].is_ascii_alphanumeric() {
                count += 1;
            }
            start = abs + keyword.len();
        }
    }

    // Logical operators: &&, ||, ??
    count += trimmed.matches("&&").count();
    count += trimmed.matches("||").count();
    count += trimmed.matches("??").count();

    // Ternary: count `?` that is not `??` and not `?.`
    let bytes = trimmed.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'?' {
            // Skip if part of ?? or ?.
            if i + 1 < bytes.len() && (bytes[i + 1] == b'?' || bytes[i + 1] == b'.') {
                continue;
            }
            // Skip if preceded by another ?
            if i > 0 && bytes[i - 1] == b'?' {
                continue;
            }
            count += 1;
        }
    }

    count
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        let mut func_start_line: Option<usize> = None;
        let mut func_name = String::new();
        let mut brace_depth: i32 = 0;
        let mut func_brace_depth: i32 = 0;
        let mut complexity: usize = 0;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Detect function start
            if func_start_line.is_none()
                && (trimmed.contains("function ")
                    || trimmed.contains("=> {")
                    || (trimmed.ends_with('{') && trimmed.contains('(') && trimmed.contains(')')))
            {
                if let Some(pos) = trimmed.find("function ") {
                    let after = &trimmed[pos + 9..];
                    func_name = after
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                        .collect();
                } else {
                    func_name = format!("<anonymous>@L{}", idx + 1);
                }
                func_start_line = Some(idx);
                func_brace_depth = brace_depth;
                complexity = 1; // base path
            }

            // Count complexity inside function
            if func_start_line.is_some() {
                complexity += line_complexity(trimmed);
            }

            // Track brace depth
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }

            // Check if function ended
            if func_start_line.is_some() && brace_depth <= func_brace_depth {
                if complexity > THRESHOLD {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: func_start_line.unwrap() + 1,
                        column: 1,
                        rule_id: "cyclomatic-complexity".into(),
                        message: format!(
                            "Function `{}` has a cyclomatic complexity of {} (max: {}).",
                            func_name, complexity, THRESHOLD
                        ),
                        severity: Severity::Warning,
                    });
                }
                func_start_line = None;
                complexity = 0;
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
    fn allows_simple_function() {
        let src = r#"
function simple() {
    if (a) {
        return 1;
    }
    return 2;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_complex_function() {
        // 1 base + 11 if = 12 complexity
        let src = r#"
function complex(x) {
    if (a) {}
    if (b) {}
    if (c) {}
    if (d) {}
    if (e) {}
    if (f) {}
    if (g) {}
    if (h) {}
    if (i) {}
    if (j) {}
    if (k) {}
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("12"));
    }

    #[test]
    fn counts_logical_operators() {
        // 1 base + 1 if + 5 && = 7 — under threshold
        let src = r#"
function check(a, b, c, d, e, f) {
    if (a && b && c && d && e) {
        return true;
    }
    return false;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn counts_ternary() {
        // 1 base + 11 ternaries = 12
        let src = r#"
function ternaries(x) {
    const a = x ? 1 : 0;
    const b = x ? 1 : 0;
    const c = x ? 1 : 0;
    const d = x ? 1 : 0;
    const e = x ? 1 : 0;
    const f = x ? 1 : 0;
    const g = x ? 1 : 0;
    const h = x ? 1 : 0;
    const i = x ? 1 : 0;
    const j = x ? 1 : 0;
    const k = x ? 1 : 0;
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
