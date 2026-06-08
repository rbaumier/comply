//! regex-no-useless-dollar-replacements OXC backend.
//!
//! Visits `RegExpLiteral` nodes inside `.replace()` / `.replaceAll()` calls.
//! Flags when the replacement string contains `$N` references exceeding the
//! number of capturing groups in the regex.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const REPLACE_METHODS: &[&str] = &["replace", "replaceAll"];

/// Count capturing groups in a regex pattern. Non-capturing groups `(?:...)`,
/// lookarounds `(?=...)` / `(?!...)` / `(?<=...)` / `(?<!...)` and named
/// groups `(?<name>...)` (still capturing) are handled correctly. Escaped
/// parens and parens inside character classes are ignored.
fn count_capturing_groups(pattern: &str) -> usize {
    let bytes = pattern.as_bytes();
    let mut groups = 0usize;
    let mut i = 0;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                i += 2;
                continue;
            }
            b'[' if !in_class => {
                in_class = true;
                i += 1;
                continue;
            }
            b']' if in_class => {
                in_class = false;
                i += 1;
                continue;
            }
            b'(' if !in_class => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                    if i + 2 < bytes.len()
                        && bytes[i + 2] == b'<'
                        && i + 3 < bytes.len()
                        && bytes[i + 3] != b'='
                        && bytes[i + 3] != b'!'
                    {
                        groups += 1;
                    }
                } else {
                    groups += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    groups
}

/// Scan a replacement string for the highest `$N` numeric reference.
fn max_dollar_numeric_ref(replacement: &str) -> usize {
    let bytes = replacement.as_bytes();
    let mut max_ref = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'$' {
                i += 2;
                continue;
            }
            if next.is_ascii_digit() {
                let mut n = (next - b'0') as usize;
                let mut consumed = 2;
                if i + 2 < bytes.len() && bytes[i + 2].is_ascii_digit() {
                    let two = n * 10 + (bytes[i + 2] - b'0') as usize;
                    if n != 0 {
                        n = two;
                        consumed = 3;
                    }
                }
                if n > max_ref {
                    max_ref = n;
                }
                i += consumed;
                continue;
            }
        }
        i += 1;
    }
    max_ref
}

/// Extract the static string value from an expression, if it's a string literal
/// or a template literal with no substitutions.
fn static_string_value<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(s) => Some(s.value.as_str()),
        Expression::TemplateLiteral(t) => {
            if !t.expressions.is_empty() {
                return None;
            }
            // All quasis concatenated
            if t.quasis.len() == 1 {
                Some(t.quasis[0].value.raw.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".replace"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `<expr>.<method>` where method is replace/replaceAll
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !REPLACE_METHODS.contains(&method) {
            return;
        }

        // Need at least 2 arguments: regex and replacement
        if call.arguments.len() < 2 {
            return;
        }

        // First argument must be a regex literal
        let Argument::RegExpLiteral(regex) = &call.arguments[0] else {
            return;
        };

        // Second argument must be a static string
        let replacement_expr = match &call.arguments[1] {
            Argument::StringLiteral(s) => {
                let max_ref = max_dollar_numeric_ref(s.value.as_str());
                if max_ref == 0 {
                    return;
                }
                let group_count = count_capturing_groups(regex.regex.pattern.text.as_str());
                if max_ref <= group_count {
                    return;
                }
                regex.span
            }
            Argument::TemplateLiteral(t) => {
                if !t.expressions.is_empty() {
                    return;
                }
                let text = if t.quasis.len() == 1 {
                    t.quasis[0].value.raw.as_str()
                } else {
                    return;
                };
                let max_ref = max_dollar_numeric_ref(text);
                if max_ref == 0 {
                    return;
                }
                let group_count = count_capturing_groups(regex.regex.pattern.text.as_str());
                if max_ref <= group_count {
                    return;
                }
                regex.span
            }
            other => {
                let expr = match other {
                    Argument::ParenthesizedExpression(p) => &p.expression,
                    _ => {
                        let Some(expr) = other.as_expression() else {
                            return;
                        };
                        expr
                    }
                };
                let Some(text) = static_string_value(expr) else {
                    return;
                };
                let max_ref = max_dollar_numeric_ref(text);
                if max_ref == 0 {
                    return;
                }
                let group_count = count_capturing_groups(regex.regex.pattern.text.as_str());
                if max_ref <= group_count {
                    return;
                }
                regex.span
            }
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, replacement_expr.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Replacement string references a capturing group that does not exist in the regex.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_nonexistent_group_ref() {
        assert_eq!(run_on(r#"str.replace(/(a)/, "$2");"#).len(), 1);
    }


    #[test]
    fn allows_valid_group_ref() {
        assert!(run_on(r#"str.replace(/(a)/, "$1");"#).is_empty());
    }


    #[test]
    fn flags_replaceall_nonexistent() {
        assert_eq!(run_on(r#"str.replaceAll(/(a)/g, "$3");"#).len(), 1);
    }


    #[test]
    fn allows_no_groups_no_refs() {
        assert!(run_on(r#"str.replace(/a/, "b");"#).is_empty());
    }


    #[test]
    fn allows_escaped_dollar() {
        // `$$` is a literal dollar sign, not a numeric reference.
        assert!(run_on(r#"str.replace(/(a)/, "$$1");"#).is_empty());
    }


    #[test]
    fn ignores_non_capturing_groups() {
        // `(?:...)` doesn't contribute to the group count.
        assert_eq!(run_on(r#"str.replace(/(?:a)/, "$1");"#).len(), 1);
    }


    #[test]
    fn respects_named_capturing_groups() {
        // `(?<name>...)` is still a capturing group (referenceable as `$1`).
        assert!(run_on(r#"str.replace(/(?<name>a)/, "$1");"#).is_empty());
    }


    #[test]
    fn ignores_lookahead_groups() {
        // `(?=...)` is not capturing.
        assert_eq!(run_on(r#"str.replace(/(?=a)/, "$1");"#).len(), 1);
    }


    #[test]
    fn flags_template_literal_replacement() {
        assert_eq!(run_on(r#"str.replace(/(a)/, `$2`);"#).len(), 1);
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a/b";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
