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
        // Skip unions that are the body of a named type alias (`type Foo = A | B
        // | …`). The diagnostic advises "extract a type alias", which is moot
        // once the union is already named — exhaustive domain unions (ApiError,
        // AuthorizeIntent, closed literal sets) are the canonical representation.
        // Inline unions in annotations/params stay flagged.
        if matches!(
            semantic.nodes().kind(parent_id),
            AstKind::TSTypeAliasDeclaration(_)
        ) {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_large_named_union_alias_issue_588() {
        // Exhaustive domain unions (ApiError, AuthorizeIntent) and closed literal
        // sets (ToastPosition) are named type aliases — already the canonical
        // representation; "extract a type alias" is moot.
        let src = "type ApiError = A | B | C | D | E | F | G;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_large_literal_union_alias_issue_588() {
        let src = r#"type ToastPosition = "top-left" | "top-center" | "top-right" | "bottom-left" | "bottom-center" | "bottom-right";"#;
        assert!(run_on(src).is_empty());
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

    /// Issue #1304: a large union inside a `test-d/` type-assertion file is the
    /// expected type being verified, not a maintainability smell — skip via the
    /// central `in_test_dir` gate (`skip_in_test_dir = true`).
    #[test]
    fn skips_large_union_in_type_test_dir_issue_1304() {
        let src = "function foo(x: A | B | C | D | E | F) {}";
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "test-d/replace.ts");
        assert!(diags.is_empty(), "expected no diagnostics in test-d/, got {diags:?}");
    }

    /// Negative-space guard: the same large union in production code is still
    /// flagged once the test-dir gate is applied.
    #[test]
    fn flags_large_union_in_production_path_issue_1304() {
        let src = "function foo(x: A | B | C | D | E | F) {}";
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/types.ts");
        assert_eq!(diags.len(), 1, "expected 1 diagnostic in src/, got {diags:?}");
    }
}
