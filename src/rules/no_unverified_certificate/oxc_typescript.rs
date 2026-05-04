use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

const FALSY_REJECT_KEYS: &[&str] = &["rejectUnauthorized", "verify"];

fn is_false_literal(expr: &Expression) -> bool {
    match expr {
        Expression::BooleanLiteral(b) => !b.value,
        Expression::StringLiteral(s) => {
            let inner = s.value.as_str();
            inner == "0" || inner.eq_ignore_ascii_case("false")
        }
        _ => false,
    }
}

fn emit(ctx: &CheckCtx, start: u32, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Disabled SSL certificate verification — enables MITM attacks.".into(),
        severity: super::META.severity,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "rejectUnauthorized",
            "NODE_TLS_REJECT_UNAUTHORIZED",
            "verify",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ObjectProperty(prop) => {
                let key_name = match &prop.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => return,
                };
                if !FALSY_REJECT_KEYS.contains(&key_name) {
                    return;
                }
                if !is_false_literal(&prop.value) {
                    return;
                }
                emit(ctx, prop.span.start, diagnostics);
            }
            AstKind::AssignmentExpression(assign) => {
                let lhs_text = match &assign.left {
                    AssignmentTarget::StaticMemberExpression(m) => {
                        &ctx.source[m.span.start as usize..m.span.end as usize]
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        &ctx.source[m.span.start as usize..m.span.end as usize]
                    }
                    _ => return,
                };
                if !lhs_text.contains("NODE_TLS_REJECT_UNAUTHORIZED") {
                    return;
                }
                emit(ctx, assign.span.start, diagnostics);
            }
            _ => {}
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
    fn flags_reject_unauthorized_false() {
        assert_eq!(run_on("const x = { rejectUnauthorized: false };").len(), 1);
    }

    #[test]
    fn flags_node_tls_env() {
        assert_eq!(
            run_on("process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'").len(),
            1
        );
    }

    #[test]
    fn flags_verify_false() {
        assert_eq!(run_on("const x = { verify: false };").len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        assert!(run_on("const x = { rejectUnauthorized: true };").is_empty());
    }

    #[test]
    fn allows_verify_true() {
        assert!(run_on("const x = { verify: true };").is_empty());
    }
}
