//! no-double-cast OXC backend — flag `x as unknown as T` style double casts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        // Double casts are the standard pattern for test doubles / partial stubs.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // The inner expression of `x as A as B` is itself a TSAsExpression.
        let Expression::TSAsExpression(inner) = &as_expr.expression else {
            return;
        };

        // `x as unknown as T` is the canonical contravariant-boundary escape
        // hatch (TypeScript itself recommends it for genuine type-erasure
        // cases). Flagging it produces noise on TanStack Router / library
        // bridges where the user has no other option.
        // Only skip if the inner cast's own expression is NOT itself a
        // TSAsExpression — a triple cast `((x as A) as unknown) as B` still
        // contains a real `as A as unknown` inner pair that should fire.
        if matches!(inner.type_annotation, TSType::TSUnknownKeyword(_))
            && !matches!(inner.expression, Expression::TSAsExpression(_))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Double cast `as X as Y` hides misaligned types. \
                      Fix the real problem: align the interface, or \
                      validate at the boundary with a type guard or Zod \
                      schema that actually checks the shape at runtime."
                .into(),
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
    fn flags_as_any_as_t() {
        assert_eq!(run_on("const x = value as any as User;").len(), 1);
    }

    #[test]
    fn allows_as_unknown_as_t() {
        // `as unknown as T` is the canonical contravariant-boundary escape
        // hatch — required by TanStack Router etc. for generic type bridging.
        let src = "const navigate = routeApi.useNavigate() as unknown as \
                   (options: { search: (p: TSearch) => TSearch }) => void;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("const x = value as MyType;").is_empty());
    }
}
