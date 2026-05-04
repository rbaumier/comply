//! OXC backend for prefer-string-starts-ends-with — flag `/^simple/.test()` and `/simple$/.test()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const SPECIAL_CHARS: &[char] = &['^', '$', '+', '[', '{', '(', '\\', '.', '?', '*', '|'];

fn is_simple_string(s: &str) -> bool {
    !s.chars().any(|c| SPECIAL_CHARS.contains(&c))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `<regex>.test`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "test" {
            return;
        }

        // Object must be a regex literal
        let Expression::RegExpLiteral(regex) = &member.object else { return };

        // Extract pattern and flags from the source text `/pattern/flags`
        let re_src = &ctx.source[regex.span.start as usize..regex.span.end as usize];
        let inner = re_src.strip_prefix('/').unwrap_or(re_src);
        let Some(last_slash) = inner.rfind('/') else { return };
        let pattern = &inner[..last_slash];
        let flags = &inner[last_slash + 1..];

        // Skip if flags contain `i` or `m`
        if flags.contains('i') || flags.contains('m') {
            return;
        }

        if pattern.is_empty() {
            return;
        }

        // Check for ^prefix pattern
        if let Some(literal) = pattern.strip_prefix('^')
            && is_simple_string(literal) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "prefer-string-starts-ends-with".into(),
                    message: "Prefer `String#startsWith()` over a regex with `^`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }

        // Check for suffix$ pattern
        if let Some(literal) = pattern.strip_suffix('$')
            && is_simple_string(literal) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "prefer-string-starts-ends-with".into(),
                    message: "Prefer `String#endsWith()` over a regex with `$`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_starts_with_regex() {
        let d = run_on(r#"/^foo/.test(str)"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("startsWith"));
    }

    #[test]
    fn flags_ends_with_regex() {
        let d = run_on(r#"/bar$/.test(str)"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("endsWith"));
    }

    #[test]
    fn ignores_case_insensitive() {
        assert!(run_on(r#"/^foo/i.test(str)"#).is_empty());
    }

    #[test]
    fn ignores_multiline() {
        assert!(run_on(r#"/^foo/m.test(str)"#).is_empty());
    }

    #[test]
    fn allows_complex_regex() {
        assert!(run_on(r#"/^fo+o/.test(str)"#).is_empty());
    }

    #[test]
    fn allows_non_test_call() {
        assert!(run_on(r#"/^foo/.exec(str)"#).is_empty());
    }
}
