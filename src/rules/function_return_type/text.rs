use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralType {
    Number,
    String,
    Boolean,
    Null,
    Undefined,
    Array,
    Object,
}

/// Classify the literal type of a return value, if recognizable.
fn classify_return_value(value: &str) -> Option<LiteralType> {
    let v = value.trim().trim_end_matches(';').trim();
    if v.is_empty() {
        return None;
    }
    if v == "null" {
        return Some(LiteralType::Null);
    }
    if v == "undefined" {
        return Some(LiteralType::Undefined);
    }
    if v == "true" || v == "false" {
        return Some(LiteralType::Boolean);
    }
    if v.starts_with('"') || v.starts_with('\'') || v.starts_with('`') {
        return Some(LiteralType::String);
    }
    if v.starts_with('[') {
        return Some(LiteralType::Array);
    }
    if v.starts_with('{') {
        return Some(LiteralType::Object);
    }
    // Number: starts with digit or negative number
    let numeric_candidate = v.strip_prefix('-').unwrap_or(v);
    if numeric_candidate.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(LiteralType::Number);
    }
    // Not a recognizable literal (variable, function call, etc.)
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Track function boundaries by brace depth
        let mut func_start_line: Option<usize> = None;
        let mut func_name = String::new();
        let mut brace_depth: i32 = 0;
        let mut func_brace_depth: i32 = 0;
        let mut return_types: Vec<(usize, LiteralType)> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Detect function start
            if func_start_line.is_none()
                && (trimmed.contains("function ")
                    || trimmed.contains("=> {")
                    || (trimmed.ends_with('{') && (trimmed.contains("(") && trimmed.contains(")"))))
            {
                // Extract a rough function name
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
                return_types.clear();
            }

            // Track brace depth
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }

            // Collect return statements inside a function
            if func_start_line.is_some()
                && let Some(ret_pos) = trimmed.find("return ") {
                    let value = &trimmed[ret_pos + 7..];
                    if let Some(lit_type) = classify_return_value(value) {
                        return_types.push((idx + 1, lit_type));
                    }
                }

            // Check if function ended
            if func_start_line.is_some() && brace_depth <= func_brace_depth {
                // Function closed — check collected returns
                let distinct: Vec<LiteralType> = {
                    let mut types: Vec<LiteralType> = return_types.iter().map(|r| r.1).collect();
                    types.dedup();
                    types.sort_by_key(|t| *t as u8);
                    types.dedup();
                    types
                };
                if distinct.len() > 1 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: func_start_line.unwrap() + 1,
                        column: 1,
                        rule_id: "function-return-type".into(),
                        message: format!(
                            "Function `{}` returns literals of different types.",
                            func_name
                        ),
                        severity: Severity::Warning,
                    });
                }
                func_start_line = None;
                return_types.clear();
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
    fn flags_mixed_return_types() {
        let src = r#"
function foo() {
    if (true) {
        return 5;
    }
    return "text";
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_consistent_return_types() {
        let src = r#"
function foo() {
    if (true) {
        return 1;
    }
    return 2;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_number_and_boolean() {
        let src = r#"
function check(x) {
    if (x) {
        return true;
    }
    return 0;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_literal_returns() {
        let src = r#"
function bar() {
    if (true) {
        return compute();
    }
    return getValue();
}
"#;
        assert!(run(src).is_empty());
    }
}
