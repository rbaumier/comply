//! function-return-type backend — flag functions returning literals of mixed types.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

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
    let numeric_candidate = v.strip_prefix('-').unwrap_or(v);
    if numeric_candidate.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(LiteralType::Number);
    }
    None
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        let mut func_start_line: Option<usize> = None;
        let mut func_name = String::new();
        let mut brace_depth: i32 = 0;
        let mut func_brace_depth: i32 = 0;
        let mut return_types: Vec<(usize, LiteralType)> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if func_start_line.is_none()
                && (trimmed.contains("function ")
                    || trimmed.contains("=> {")
                    || (trimmed.ends_with('{')
                        && (trimmed.contains('(') && trimmed.contains(')'))))
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
                return_types.clear();
            }

            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }

            if func_start_line.is_some()
                && let Some(ret_pos) = trimmed.find("return ")
            {
                let value = &trimmed[ret_pos + 7..];
                if let Some(lit_type) = classify_return_value(value) {
                    return_types.push((idx + 1, lit_type));
                }
            }

            if func_start_line.is_some() && brace_depth <= func_brace_depth {
                let distinct: Vec<LiteralType> = {
                    let mut types: Vec<LiteralType> =
                        return_types.iter().map(|r| r.1).collect();
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
                        span: None,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_mixed_return_types() {
        let src = "function foo() {\n    if (true) {\n        return 5;\n    }\n    return \"text\";\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_consistent_return_types() {
        let src = "function foo() {\n    if (true) {\n        return 1;\n    }\n    return 2;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_literal_returns() {
        let src = "function bar() {\n    if (true) {\n        return compute();\n    }\n    return getValue();\n}";
        assert!(run_on(src).is_empty());
    }
}
