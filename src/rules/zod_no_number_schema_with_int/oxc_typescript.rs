use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.number"])
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
        // Outer call must be `<expr>.int()`
        let oxc_ast::ast::Expression::StaticMemberExpression(outer_member) = &call.callee else {
            return;
        };
        if outer_member.property.name.as_str() != "int" {
            return;
        }
        // Inner object must be `z.number()`
        let oxc_ast::ast::Expression::CallExpression(inner_call) = &outer_member.object else {
            return;
        };
        let oxc_ast::ast::Expression::StaticMemberExpression(inner_member) =
            &inner_call.callee
        else {
            return;
        };
        let oxc_ast::ast::Expression::Identifier(obj) = &inner_member.object else {
            return;
        };
        if obj.name.as_str() != "z" || inner_member.property.name.as_str() != "number" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.number().int()` can be replaced by `z.int()` in Zod v4+.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_z_number_int() {
        assert_eq!(run("const s = z.number().int();").len(), 1);
    }

    #[test]
    fn allows_z_int() {
        assert!(run("const s = z.int();").is_empty());
    }

    #[test]
    fn allows_z_number_positive() {
        assert!(run("const s = z.number().positive();").is_empty());
    }
}
