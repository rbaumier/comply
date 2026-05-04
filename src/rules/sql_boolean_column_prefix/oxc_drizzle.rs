use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "boolean" {
            return;
        }
        for arg in &call.arguments {
            if let Argument::StringLiteral(lit) = arg {
                let col_name = lit.value.as_str();
                let lower = col_name.to_ascii_lowercase();
                if !lower.starts_with("is_") && !lower.starts_with("has_") {
                    let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "BOOLEAN column `{col_name}` should be prefixed with \
                             `is_` or `has_` — the prefix makes boolean semantics \
                             obvious at call sites."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_boolean_active() {
        let src = "const active = boolean('active');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_boolean_admin() {
        let src = "const admin = boolean('admin');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_is_prefix() {
        let src = "const isActive = boolean('is_active');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_has_prefix() {
        let src = "const hasRole = boolean('has_role');";
        assert!(run_on(src).is_empty());
    }
}
