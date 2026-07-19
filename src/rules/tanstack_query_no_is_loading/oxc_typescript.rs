//! tanstack-query-no-is-loading oxc backend.
//!
//! Flag `isLoading` only when it reads a `useMutation()` result. TanStack Query
//! v5 renamed `isLoading` → `isPending` on `UseMutationResult`; query results
//! (`useQuery`, `useInfiniteQuery`, `useSuspenseQuery`, …) still expose
//! `isLoading` (now derived as `isPending && isFetching`), so those reads are
//! valid and must not fire.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

const MESSAGE: &str =
    "`isLoading` was removed from `useMutation` results in TanStack Query v5 — use `isPending` instead.";

/// True when `call`'s callee is a bare identifier that resolves to the
/// `useMutation` binding imported from `@tanstack/react-query`, alias-aware:
/// `import { useMutation as useMut }` still matches via the module-side name.
/// A local `function useMutation()` or a `useMutation` imported from another
/// module resolves to a different declaration and does not match, so only the
/// real TanStack Query mutation hook is recognized.
///
/// Resolves the callee via `reference_id` → symbol → declaration node, then
/// walks the declaration and its ancestors for the `ImportSpecifier` (module-side
/// name) and its enclosing `ImportDeclaration` (source module).
fn callee_is_tanstack_use_mutation<'a>(
    call: &oxc_ast::ast::CallExpression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    let Some(ref_id) = callee.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    let mut imported_is_use_mutation = false;
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::ImportSpecifier(spec) => {
                imported_is_use_mutation = spec.imported.name().as_str() == "useMutation";
            }
            AstKind::ImportDeclaration(import) => {
                return imported_is_use_mutation
                    && import.source.value.as_str() == "@tanstack/react-query";
            }
            _ => {}
        }
    }
    false
}

/// True when `id` resolves to a `const`/`let` binding initialized directly by a
/// TanStack Query `useMutation()` call — the `const m = useMutation(...)` half of
/// `m.isLoading`. Resolves the reference via `reference_id` → symbol → the
/// enclosing `VariableDeclarator`'s initializer.
fn receiver_resolves_to_use_mutation<'a>(
    id: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return matches!(
                decl.init.as_ref(),
                Some(Expression::CallExpression(call)) if callee_is_tanstack_use_mutation(call, semantic)
            );
        }
    }
    false
}

fn make_diagnostic(ctx: &CheckCtx, offset: u32) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: MESSAGE.into(),
        severity: Severity::Error,
        span: None,
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["isLoading"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                // `const { isLoading } = useMutation(...)` and renamed
                // `const { isLoading: loading } = useMutation(...)`: the
                // destructured property reads `isLoading` off the mutation result.
                AstKind::VariableDeclarator(decl) => {
                    let Some(Expression::CallExpression(call)) = decl.init.as_ref() else {
                        continue;
                    };
                    if !callee_is_tanstack_use_mutation(call, semantic) {
                        continue;
                    }
                    let BindingPattern::ObjectPattern(pattern) = &decl.id else {
                        continue;
                    };
                    for prop in &pattern.properties {
                        if let PropertyKey::StaticIdentifier(key) = &prop.key
                            && key.name.as_str() == "isLoading"
                        {
                            diagnostics.push(make_diagnostic(ctx, key.span.start));
                        }
                    }
                }
                // `const m = useMutation(...); m.isLoading`: the member read
                // resolves its receiver back to the mutation result.
                AstKind::StaticMemberExpression(member) => {
                    if member.property.name.as_str() != "isLoading" {
                        continue;
                    }
                    let Expression::Identifier(object) = &member.object else {
                        continue;
                    };
                    if receiver_resolves_to_use_mutation(object, semantic) {
                        diagnostics.push(make_diagnostic(ctx, member.property.span.start));
                    }
                }
                _ => {}
            }
        }
        diagnostics
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
    use super::Check;

    // ── Valid v5 query usage: no diagnostic ────────────────────────────

    #[test]
    fn allows_use_query_destructured_is_loading_issue_7670() {
        // #7670 repro: `isLoading` on a `useQuery` result is valid v5.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            function C() {
                const { data, isLoading } = useQuery({ queryKey, queryFn });
                if (isLoading) return null;
                return data;
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_use_infinite_query_is_loading() {
        let src = r#"
            import { useInfiniteQuery } from "@tanstack/react-query";
            const { isLoading } = useInfiniteQuery({ queryKey, queryFn });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_use_suspense_query_is_loading() {
        let src = r#"
            import { useSuspenseQuery } from "@tanstack/react-query";
            const { isLoading } = useSuspenseQuery({ queryKey, queryFn });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_query_is_loading_in_file_that_also_uses_mutation() {
        // The query's `isLoading` must stay silent even when a `useMutation`
        // lives in the same file — provenance is per-binding, not per-file.
        let src = r#"
            import { useQuery, useMutation } from "@tanstack/react-query";
            function C() {
                const { isLoading } = useQuery({ queryKey, queryFn });
                const { mutate } = useMutation({ mutationFn });
                return isLoading;
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_local_use_mutation_not_from_tanstack() {
        // A same-named local `useMutation` gives no v5 guarantee; its
        // `isLoading` is not the removed field.
        let src = r#"
            function useMutation() { return { isLoading: false }; }
            const { isLoading } = useMutation();
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_use_mutation_imported_from_other_module() {
        // A `useMutation` from a custom wrapper module is not the tanstack hook,
        // so its `isLoading` carries no v5 removal — import-provenance, not name.
        let src = r#"
            import { useMutation } from "./hooks";
            const { isLoading } = useMutation({ mutationFn });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_query_result_member_access() {
        // Member access on a `useQuery` result reads a field that still exists
        // in v5; the receiver resolves to a query, not a mutation.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            const q = useQuery({ queryKey, queryFn });
            if (q.isLoading) doThing();
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    // ── The real removal: useMutation results get flagged ──────────────

    #[test]
    fn flags_use_mutation_destructured_is_loading() {
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            const { isLoading } = useMutation({ mutationFn });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
    }

    #[test]
    fn flags_use_mutation_renamed_destructure() {
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            const { isLoading: loading } = useMutation({ mutationFn });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
    }

    #[test]
    fn flags_use_mutation_member_access() {
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            const m = useMutation({ mutationFn });
            if (m.isLoading) doThing();
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
    }

    #[test]
    fn flags_import_aliased_use_mutation() {
        let src = r#"
            import { useMutation as useMut } from "@tanstack/react-query";
            const { isLoading } = useMut({ mutationFn });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
    }
}
