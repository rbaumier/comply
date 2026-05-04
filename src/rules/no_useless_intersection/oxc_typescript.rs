use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSIntersectionType]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSIntersectionType(intersection) = node.kind() else {
            return;
        };
        let has_useless = intersection.types.iter().any(|ty| {
            matches!(ty, TSType::TSUnknownKeyword(_) | TSType::TSNeverKeyword(_))
        });
        if !has_useless {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, intersection.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Intersection with `unknown` or `never` is useless — simplify it.".into(),
            severity: super::META.severity,
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
    fn flags_intersection_with_unknown() {
        assert_eq!(run_on("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_unknown_on_left() {
        assert_eq!(run_on("type X = unknown & Foo;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_never() {
        assert_eq!(run_on("type X = Foo & never;").len(), 1);
    }

    #[test]
    fn allows_intersection_with_any() {
        assert!(run_on("type X = Foo & any;").is_empty());
    }

    #[test]
    fn allows_any_on_left() {
        assert!(run_on("type X = any & Foo;").is_empty());
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run_on("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        assert!(run_on("type X = anything & Foo;").is_empty());
    }
}
