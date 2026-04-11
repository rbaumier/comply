use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Checks whether `name` at position `pos` in `line` is a standalone identifier
/// (not part of a member expression like `Number.isNaN` or `foo.isNaN`).
fn is_standalone_call(line: &str, pos: usize, name: &str) -> bool {
    // Check character before: must not be `.` or alphanumeric/underscore
    if pos > 0 {
        let before = line.as_bytes()[pos - 1];
        if before == b'.' || before == b'_' || before.is_ascii_alphanumeric() {
            return false;
        }
    }

    // Check character after the name: must be `(` for calls, or non-alnum for NaN/Infinity
    let end = pos + name.len();
    if end < line.len() {
        let after = line.as_bytes()[end];
        if after == b'_' || after.is_ascii_alphanumeric() {
            return false;
        }
    }

    true
}

/// Finds the first standalone occurrence of `name` in `line`, skipping
/// occurrences inside string literals and comments.
fn find_standalone(line: &str, name: &str) -> Option<usize> {
    let stripped = strip_strings_and_comments(line);
    let mut start = 0;
    while let Some(pos) = stripped[start..].find(name) {
        let abs_pos = start + pos;
        if is_standalone_call(&stripped, abs_pos, name) {
            return Some(abs_pos);
        }
        start = abs_pos + 1;
    }
    None
}

/// Strip string literals and single-line comments.
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

struct GlobalCheck {
    global_name: &'static str,
    is_call: bool,
    message: &'static str,
}

const CHECKS: &[GlobalCheck] = &[
    GlobalCheck {
        global_name: "isNaN",
        is_call: true,
        message: "Prefer `Number.isNaN()` over global `isNaN()`. `Number.isNaN()` does not coerce.",
    },
    GlobalCheck {
        global_name: "isFinite",
        is_call: true,
        message: "Prefer `Number.isFinite()` over global `isFinite()`. `Number.isFinite()` does not coerce.",
    },
    GlobalCheck {
        global_name: "parseInt",
        is_call: true,
        message: "Prefer `Number.parseInt()` over global `parseInt()`.",
    },
    GlobalCheck {
        global_name: "parseFloat",
        is_call: true,
        message: "Prefer `Number.parseFloat()` over global `parseFloat()`.",
    },
    GlobalCheck {
        global_name: "NaN",
        is_call: false,
        message: "Prefer `Number.NaN` over global `NaN`.",
    },
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for chk in CHECKS {
                if !line.contains(chk.global_name) {
                    continue;
                }
                if let Some(_pos) = find_standalone(line, chk.global_name) {
                    // For calls, verify there's a `(` after the name
                    if chk.is_call {
                        let after_name = &line[_pos + chk.global_name.len()..];
                        let after_trimmed = after_name.trim_start();
                        if !after_trimmed.starts_with('(') {
                            continue;
                        }
                    }
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "prefer-number-properties".into(),
                        message: chk.message.into(),
                        severity: Severity::Warning,
                    });
                }
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
    fn flags_global_is_nan() {
        let d = run("if (isNaN(value)) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-number-properties");
        assert!(d[0].message.contains("Number.isNaN"));
    }

    #[test]
    fn flags_global_parse_int() {
        let d = run("const n = parseInt('10', 10);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number.parseInt"));
    }

    #[test]
    fn flags_global_parse_float() {
        let d = run("const n = parseFloat('3.14');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_global_is_finite() {
        let d = run("if (isFinite(x)) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_global_nan() {
        let d = run("const x = NaN;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number.NaN"));
    }

    #[test]
    fn allows_number_is_nan() {
        assert!(run("if (Number.isNaN(value)) {}").is_empty());
    }

    #[test]
    fn allows_number_parse_int() {
        assert!(run("const n = Number.parseInt('10', 10);").is_empty());
    }

    #[test]
    fn ignores_member_access() {
        // `foo.isNaN` should not be flagged
        assert!(run("foo.isNaN(value);").is_empty());
    }

    #[test]
    fn ignores_string_literal() {
        assert!(run(r#"const s = "isNaN(value)";"#).is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run("// isNaN(value)").is_empty());
    }
}
