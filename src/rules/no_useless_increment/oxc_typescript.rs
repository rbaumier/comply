//! no-useless-increment — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ReturnStatement(ret) = node.kind() else { return };

        let Some(arg) = &ret.argument else { return };
        let Expression::UpdateExpression(update) = arg else { return };

        // Only flag postfix (`x++` / `x--`), not prefix (`++x`).
        if update.prefix {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`return x++` / `return x--` returns the value before the mutation — use prefix or separate statements.".into(),
            severity: Severity::Error,
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
    fn flags_return_post_increment() {
        assert_eq!(run_on("return x++;").len(), 1);
    }


    #[test]
    fn flags_return_post_decrement() {
        assert_eq!(run_on("return count--;").len(), 1);
    }


    #[test]
    fn allows_prefix_increment() {
        assert!(run_on("return ++x;").is_empty());
    }


    #[test]
    fn allows_plain_return() {
        assert!(run_on("return x;").is_empty());
    }
}
