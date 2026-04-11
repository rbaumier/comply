use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const CONSOLE_METHODS: &[&str] = &[
    "console.log(",
    "console.debug(",
    "console.info(",
    "console.warn(",
    "console.error(",
];

#[derive(Debug)]
pub struct Check;

/// Check if a string value has a leading single space.
fn has_leading_space(val: &str) -> bool {
    val.len() > 1 && val.starts_with(' ') && !val.starts_with("  ")
}

/// Check if a string value has a trailing single space.
fn has_trailing_space(val: &str) -> bool {
    val.len() > 1 && val.ends_with(' ') && !val.ends_with("  ")
}

/// Find a console method call in the line. Returns the byte offset of the
/// opening paren if found.
fn find_console_call(line: &str) -> Option<usize> {
    for method in CONSOLE_METHODS {
        if let Some(pos) = line.find(method) {
            return Some(pos + method.len() - 1); // position of '('
        }
    }
    None
}

/// Extract string literal arguments from a console call on a single line.
/// Returns (value_without_quotes, is_first, is_last) for each string arg.
fn extract_string_args(args_str: &str) -> Vec<(String, bool, bool)> {
    let mut results = Vec::new();
    let mut i = 0;
    let bytes = args_str.as_bytes();
    let mut arg_index: usize = 0;
    let mut total_args: usize = 0;

    // First pass: count arguments (simplified — just count top-level commas)
    let mut depth = 0i32;
    for &b in bytes {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b',' if depth == 0 => total_args += 1,
            _ => {}
        }
    }
    total_args += 1; // number of args = commas + 1

    // Second pass: extract string literals
    depth = 0;
    while i < bytes.len() {
        match bytes[i] {
            b')' if depth == 0 => break,
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth -= 1;
                i += 1;
            }
            b',' if depth == 0 => {
                arg_index += 1;
                i += 1;
            }
            q @ (b'\'' | b'"' | b'`') if depth == 0 => {
                let start = i + 1;
                i += 1;
                let mut escaped = false;
                while i < bytes.len() {
                    if escaped {
                        escaped = false;
                        i += 1;
                        continue;
                    }
                    if bytes[i] == b'\\' {
                        escaped = true;
                        i += 1;
                        continue;
                    }
                    if bytes[i] == q {
                        let val = &args_str[start..i];
                        let is_first = arg_index == 0;
                        let is_last = arg_index == total_args - 1;
                        results.push((val.to_string(), is_first, is_last));
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            if let Some(paren_pos) = find_console_call(line) {
                let args_content = &line[paren_pos + 1..];
                let string_args = extract_string_args(args_content);

                for (val, is_first, is_last) in &string_args {
                    if !is_first && has_leading_space(val) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "no-console-spaces".into(),
                            message: "Do not use leading space between `console` parameters."
                                .into(),
                            severity: Severity::Warning,
                        });
                    }
                    if !is_last && has_trailing_space(val) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "no-console-spaces".into(),
                            message: "Do not use trailing space between `console` parameters."
                                .into(),
                            severity: Severity::Warning,
                        });
                    }
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
    fn flags_trailing_space_in_first_arg() {
        let d = run(r#"console.log("val: ", x);"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trailing"));
    }

    #[test]
    fn flags_leading_space_in_last_arg() {
        let d = run(r#"console.log(x, " val");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("leading"));
    }

    #[test]
    fn flags_both_leading_and_trailing() {
        let d = run(r#"console.log("a ", x, " b");"#);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_no_spaces() {
        assert!(run(r#"console.log("hello", x);"#).is_empty());
    }

    #[test]
    fn allows_single_arg_with_leading_space() {
        assert!(run(r#"console.log(" hello");"#).is_empty());
    }

    #[test]
    fn allows_single_arg_with_trailing_space() {
        assert!(run(r#"console.log("hello ");"#).is_empty());
    }

    #[test]
    fn flags_warn_method() {
        let d = run(r#"console.warn("val: ", x);"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_multiple_spaces() {
        assert!(run(r#"console.log("  hello", x);"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run(r#"// console.log("val: ", x);"#).is_empty());
    }
}
