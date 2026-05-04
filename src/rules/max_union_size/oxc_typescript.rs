use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

/// Count total leaf members in a (possibly nested) union type.
fn count_union_members(types: &oxc_allocator::Vec<'_, TSType<'_>>) -> usize {
    let mut count = 0;
    for ty in types.iter() {
        if let TSType::TSUnionType(inner) = ty {
            count += count_union_members(&inner.types);
        } else {
            count += 1;
        }
    }
    count
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSUnionType]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSUnionType(union) = node.kind() else {
            return;
        };
        // Only flag the outermost union (skip if parent is also a union).
        let parent_id = semantic.nodes().parent_id(node.id());
        if matches!(semantic.nodes().kind(parent_id), AstKind::TSUnionType(_)) {
            return;
        }
        let max = ctx.config.threshold("max-union-size", "max", ctx.lang);
        let count = count_union_members(&union.types);
        if count > max {
            let (line, column) = byte_offset_to_line_col(ctx.source, union.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Union type has {count} members (max: {max}) — consider extracting a type alias."
                ),
                severity: super::META.severity,
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
    fn flags_large_union_in_type_alias() {
        let src = "type Status = A | B | C | D | E | F;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_large_union_in_annotation() {
        let src = "function foo(x: A | B | C | D | E | F) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_small_union() {
        let src = "type Status = A | B | C;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_five_members() {
        let src = "type X = A | B | C | D | E;";
        assert!(run_on(src).is_empty());
    }
}
