use crate::diagnostic::{Diagnostic, Severity};

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
        let in_code = strip_strings_and_comments(trimmed);
        if in_code.contains("~~") {
            return Some("Use `Math.trunc(x)` instead of `~~x`.");
        }
    }

    // Binary `| 0`, `>> 0`, `<< 0`, `^ 0` and assignment forms
    let in_code = strip_strings_and_comments(trimmed);
    for pat in &["| 0", "|0", ">> 0", ">>0", "<< 0", "<<0", "^ 0", "^0"] {
        if in_code.contains(pat)
            && let Some(pos) = in_code.find(pat) {
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

    // Assignment forms: `|= 0`, `>>= 0`, `<<= 0`, `^= 0`
    for pat in &["|= 0", "|=0", ">>= 0", ">>=0", "<<= 0", "<<=0", "^= 0", "^=0"] {
        if in_code.contains(pat)
            && let Some(pos) = in_code.find(pat)
        {
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
            '/' if chars.peek() == Some(&'/') => break,
            '\'' | '"' | '`' => {
                let quote = c;
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
                result.push(' ');
            }
            _ => result.push(c),
        }
    }
    result
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
        if let Some(msg) = find_bitwise_trunc(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "prefer-math-trunc".into(),
                message: msg.into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bitwise_or_zero() {
        let d = run_ts("const n = value | 0;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-math-trunc");
    }

    #[test]
    fn flags_double_tilde() {
        let d = run_ts("const n = ~~value;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_math_trunc() {
        assert!(run_ts("const n = Math.trunc(value);").is_empty());
    }

    #[test]
    fn ignores_string_literal() {
        assert!(run_ts(r#"const s = "value | 0";"#).is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run_ts("// value | 0").is_empty());
    }
}
