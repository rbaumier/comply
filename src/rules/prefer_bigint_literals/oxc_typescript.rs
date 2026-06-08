//! prefer-bigint-literals OXC backend — flag `BigInt(123)` and `BigInt("123")`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_numeric_arg(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let s = s.trim();
    let s = s
        .strip_prefix('+')
        .or_else(|| s.strip_prefix('-'))
        .unwrap_or(s);
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    if s.len() >= 2 {
        let prefix = &s[..2].to_lowercase();
        if prefix == "0x" || prefix == "0b" || prefix == "0o" {
            return s[2..].chars().all(|c| c.is_ascii_hexdigit() || c == '_');
        }
    }
    s.chars().all(|c| c.is_ascii_digit() || c == '_')
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["BigInt"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "BigInt" {
            return;
        }

        if call.arguments.len() != 1 {
            return;
        }

        let Some(arg_expr) = call.arguments[0].as_expression() else { return };
        let arg_text = &ctx.source[arg_expr.span().start as usize..arg_expr.span().end as usize];

        let replacement = match arg_expr {
            Expression::NumericLiteral(_) => {
                if !is_numeric_arg(arg_text) { return; }
                format!("{arg_text}n")
            }
            Expression::UnaryExpression(_) => {
                if !is_numeric_arg(arg_text) { return; }
                format!("{arg_text}n")
            }
            Expression::StringLiteral(lit) => {
                let inner = lit.value.as_str().trim();
                let inner = inner.strip_prefix('+').map(|s| s.trim()).unwrap_or(inner);
                if !is_numeric_arg(inner) { return; }
                format!("{inner}n")
            }
            _ => return,
        };

        let full = &ctx.source[call.span.start as usize..call.span.end as usize];
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Prefer `{replacement}` over `{full}`."),
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
    fn flags_bigint_with_decimal() {
        let d = run_on("const x = BigInt(123);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("123n"));
    }


    #[test]
    fn flags_bigint_with_hex() {
        let d = run_on("const x = BigInt(0xFF);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFn"));
    }


    #[test]
    fn flags_bigint_with_string() {
        let d = run_on(r#"const x = BigInt("9007199254740991");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("9007199254740991n"));
    }


    #[test]
    fn allows_bigint_literal() {
        assert!(run_on("const x = 123n;").is_empty());
    }


    #[test]
    fn allows_bigint_with_variable() {
        assert!(run_on("const x = BigInt(y);").is_empty());
    }
}
