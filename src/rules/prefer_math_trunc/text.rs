use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects bitwise truncation patterns:
/// - `value | 0`  /  `value |= 0`
/// - `value >> 0` /  `value >>= 0`
/// - `value << 0` /  `value <<= 0`
/// - `value ^ 0`  /  `value ^= 0`
/// - `~~value`
fn find_bitwise_trunc(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();

    // Double bitwise NOT: `~~expr`
    if trimmed.contains("~~") {
        // Make sure it's not inside a string literal or comment.
        // Simple heuristic: if `~~` appears outside quotes, flag it.
        let in_code = strip_strings_and_comments(trimmed);
        if in_code.contains("~~") {
            return Some("Use `Math.trunc(x)` instead of `~~x`.");
        }
    }

    // Binary `| 0`, `>> 0`, `<< 0`, `^ 0` and assignment forms `|= 0`, `>>= 0`, etc.
    let in_code = strip_strings_and_comments(trimmed);
    for pat in &["| 0", "|0", ">> 0", ">>0", "<< 0", "<<0", "^ 0", "^0"] {
        if in_code.contains(pat) {
            // Make sure the 0 is at a word boundary (not part of `100` etc.)
            // Check the char after `0` — should be end, `)`, `;`, whitespace, etc.
            if let Some(pos) = in_code.find(pat) {
                let end = pos + pat.len();
                let next_char = in_code[end..].chars().next();
                match next_char {
                    None | Some(')') | Some(';') | Some(',') | Some(' ') | Some('\t') => {
                        return Some("Use `Math.trunc(x)` instead of bitwise `| 0` / `>> 0`.");
                    }
                    _ => {}
                }
            }
        }
    }

    // Assignment forms: `|= 0`, `>>= 0`, `<<= 0`, `^= 0`
    for pat in &["|= 0", "|=0", ">>= 0", ">>=0", "<<= 0", "<<=0", "^= 0", "^=0"] {
        if in_code.contains(pat)
            && let Some(pos) = in_code.find(pat) {
                let end = pos + pat.len();
                let next_char = in_code[end..].chars().next();
                match next_char {
                    None | Some(')') | Some(';') | Some(',') | Some(' ') | Some('\t') => {
                        return Some("Use `Math.trunc(x)` instead of bitwise assignment `|= 0`.");
                    }
                    _ => {}
                }
            }
    }

    None
}

/// Strip string literals and single-line comments to avoid false positives.
fn strip_strings_and_comments(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => break, // line comment
            '\'' | '"' | '`' => {
                let quote = c;
                // Skip until matching close quote
                let mut escape_next = false;
                for inner in chars.by_ref() {
                    if escape_next {
                        escape_next = false;
                        continue;
                    }
                    if inner == '\\' {
                        escape_next = true;
                    } else if inner == quote {
                        break;
                    }
                }
                result.push(' '); // placeholder
            }
            _ => result.push(c),
        }
    }
    result
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = find_bitwise_trunc(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-math-trunc".into(),
                    message: msg.into(),
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
    fn flags_bitwise_or_zero() {
        let d = run("const n = value | 0;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-math-trunc");
    }

    #[test]
    fn flags_double_tilde() {
        let d = run("const n = ~~value;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_right_shift_zero() {
        let d = run("const n = value >> 0;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_math_trunc() {
        assert!(run("const n = Math.trunc(value);").is_empty());
    }

    #[test]
    fn ignores_string_literal() {
        assert!(run(r#"const s = "value | 0";"#).is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run("// value | 0").is_empty());
    }
}
